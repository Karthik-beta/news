// Idempotency-Key handling (ADR 0006), Stripe-style:
//   - first request with a key: a 'pending' record is created (create-only put), the operation runs, and on success
//     the record becomes 'done' with the exact response;
//   - replay with the same body: the stored response is returned unchanged, and nothing is written (one GetItem);
//   - same key, different body: 409 idempotency_key_reused;
//   - same key while the first is still running: 409 idempotency_in_progress (a lock older than 2 minutes is stale
//     and may be taken over, so a crashed request does not block its retries for the whole TTL).
// A failed operation deletes its pending record, so the client's retry runs it again.

import { sha256Hex } from "@news/worker-shared/crypto";
import { and, eq, notExists } from "@news/worker-shared/dynamo/conditions";
import { ConditionFailedError, type Table } from "@news/worker-shared/dynamo/table";
import { SECURITY_HEADERS } from "@news/worker-shared/http";
import { idempotencyKey } from "@news/worker-shared/keys";
import type { IdempotencyRecord } from "@news/worker-shared/records";
import { ApiError } from "./errors";

export const IDEMPOTENCY_HEADER = "idempotency-key";
const KEY = /^[A-Za-z0-9:._-]{8,255}$/;
export const TTL_SECONDS = 7 * 24 * 3600;
const STALE_MS = 2 * 60 * 1000;

export function readKey(request: Request): string {
  const key = request.headers.get(IDEMPOTENCY_HEADER);
  if (key === null || key === "") throw new ApiError("idempotency_key_required", "Idempotency-Key header is required");
  if (!KEY.test(key))
    throw new ApiError("idempotency_key_invalid", "Idempotency-Key must be 8–255 characters of A–Z a–z 0–9 : . _ -");
  return key;
}

export interface Claim {
  keyHash: string;
  requestHash: string;
  createdAt: string;
}

export type Lookup =
  | { kind: "new"; keyHash: string; requestHash: string; existing?: IdempotencyRecord }
  | { kind: "replay"; response: Response };

/** Hash of what makes two requests "the same": method, path and exact body bytes. */
export const requestHash = (method: string, path: string, body: string) => sha256Hex(`${method} ${path}\n${body}`);

/** Read-only check, done before validation: a replay is answered without writing anything. */
export async function lookup(table: Table, key: string, reqHash: string, now: Date): Promise<Lookup> {
  const keyHash = await sha256Hex(key);
  const existing = (await table.get(idempotencyKey(keyHash))) as unknown as IdempotencyRecord | undefined;
  if (!existing || existing.expires_at * 1000 <= now.getTime())
    return { kind: "new", keyHash, requestHash: reqHash, existing };
  if (existing.request_hash !== reqHash)
    throw new ApiError("idempotency_key_reused", "this Idempotency-Key was already used with a different request");
  if (existing.state === "done" && existing.status_code !== undefined && existing.response_body !== undefined) {
    return {
      kind: "replay",
      response: new Response(existing.response_body, {
        status: existing.status_code,
        headers: {
          "content-type": "application/json; charset=utf-8",
          ...SECURITY_HEADERS,
          "idempotent-replayed": "true",
        },
      }),
    };
  }
  if (now.getTime() - Date.parse(existing.created_at) < STALE_MS)
    throw new ApiError(
      "idempotency_in_progress",
      "a request with this Idempotency-Key is still being processed",
      undefined,
      {
        "retry-after": "5",
      },
    );
  return { kind: "new", keyHash, requestHash: reqHash, existing };
}

/** Takes the lock, given what lookup() saw. Loses (409) to a concurrent request that took it first. */
export async function claim(
  table: Table,
  seen: { keyHash: string; requestHash: string; existing?: IdempotencyRecord },
  operation: string,
  now: Date,
): Promise<Claim> {
  const { keyHash, requestHash: reqHash, existing } = seen;
  const key = idempotencyKey(keyHash);
  const createdAt = now.toISOString();
  const record: IdempotencyRecord = {
    ...key,
    SK: "META",
    type: "idempotency",
    request_hash: reqHash,
    state: "pending",
    operation,
    created_at: createdAt,
    expires_at: Math.floor(now.getTime() / 1000) + TTL_SECONDS,
  };
  const expired = existing && existing.expires_at * 1000 <= now.getTime();
  const stale = existing && existing.state === "pending" && now.getTime() - Date.parse(existing.created_at) >= STALE_MS;
  try {
    // Create, or take over an expired record or a stale lock (only the exact record we saw).
    await table.put(
      record as never,
      existing && (expired || stale)
        ? and(eq("created_at", existing.created_at), eq("state", existing.state))
        : notExists(),
    );
  } catch (e) {
    if (e instanceof ConditionFailedError)
      throw new ApiError(
        "idempotency_in_progress",
        "a request with this Idempotency-Key is still being processed",
        undefined,
        {
          "retry-after": "5",
        },
      );
    throw e;
  }
  return { keyHash, requestHash: reqHash, createdAt };
}

/** Stores the successful response; later replays return exactly this status and body. */
export async function complete(table: Table, c: Claim, operation: string, now: Date, status: number, body: string) {
  const record: IdempotencyRecord = {
    ...idempotencyKey(c.keyHash),
    SK: "META",
    type: "idempotency",
    request_hash: c.requestHash,
    state: "done",
    operation,
    status_code: status,
    response_body: body,
    created_at: c.createdAt,
    expires_at: Math.floor(now.getTime() / 1000) + TTL_SECONDS,
  };
  await table.put(record as never, and(eq("state", "pending"), eq("created_at", c.createdAt)));
}

/** Releases the lock after a failure, so the retry runs the operation again. Best effort. */
export async function release(table: Table, c: Claim): Promise<void> {
  await table
    .delete(idempotencyKey(c.keyHash), and(eq("state", "pending"), eq("created_at", c.createdAt)))
    .catch(() => {});
}
