// The page lock (ADR 0004): one write operation per page at a time. Publish, withdraw and index promotion take it
// before they read the page and give it back when they are done, so two operations on one page never interleave
// their R2 files, hub entries and search row. Page files are served from R2 at the page's public path, so they are
// rewritten in place; serialising the writers is what keeps them in step with the live record.
//
// A lock is a create-only put. One older than LEASE_MS is stale (its request crashed) and may be taken over; every
// DynamoDB, R2 and D1 call has its own timeout well inside that, so a live request finishes within its lease.
import { randomHex } from "@news/worker-shared/crypto";
import { eq, lt, notExists, or } from "@news/worker-shared/dynamo/conditions";
import { ConditionFailedError, type Key, type Table } from "@news/worker-shared/dynamo/table";
import { pageLockKey } from "@news/worker-shared/keys";
import type { PageLock } from "@news/worker-shared/records";
import { ApiError } from "./errors";

export const LEASE_MS = 2 * 60 * 1000;

export interface HeldLock {
  key: Key;
  owner: string;
}

export async function lockPage(table: Table, pageId: string, now: Date): Promise<HeldLock> {
  const key = pageLockKey(pageId);
  const owner = randomHex(16);
  const item: PageLock = {
    ...key,
    SK: "LOCK",
    type: "page_lock",
    owner,
    acquired_at: now.toISOString(),
    expires_at: Math.floor(now.getTime() / 1000) + 24 * 3600,
  };
  const staleBefore = new Date(now.getTime() - LEASE_MS).toISOString();
  try {
    await table.put(item as never, or(notExists(), lt("acquired_at", staleBefore)));
  } catch (e) {
    if (e instanceof ConditionFailedError)
      throw new ApiError("page_busy", "another operation on this page is running; retry shortly", undefined, {
        "retry-after": "5",
      });
    throw e;
  }
  return { key, owner };
}

/** Gives the lock back (only our own). Best effort: a lock that stays behind goes stale after LEASE_MS. */
export async function unlockPage(table: Table, lock: HeldLock): Promise<void> {
  await table.delete(lock.key, eq("owner", lock.owner)).catch(() => {});
}
