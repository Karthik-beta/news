// The page lock (ADR 0004): one write operation per page at a time. Publish, withdraw and index promotion take it
// before they read the page and give it back when they are done, so two operations on one page never interleave
// their R2 files, hub entries and search row. Page files are served from R2 at the page's public path, so they are
// rewritten in place; serialising the writers is what keeps them in step with the live record.
//
// A lock is a create-only put. One older than LEASE_MS is stale (its request crashed) and may be taken over. A holder
// must therefore never write after its lease, and the Fence below is how (ADR 0004, "One writer per page"):
//   - a deadline, DEADLINE_MS after the lock was taken (20 s before anyone may take over): no page write starts later;
//   - every storage call has a timeout (DynamoDB 8 s in its client, R2 and D1 `timeoutMs` here), so a call started
//     before the deadline is answered or abandoned before the lease ends; an abandoned write is an unknown outcome,
//     and then the lock is kept until it is stale instead of being given back. A batch write gets the deadline too,
//     so its retries of unprocessed writes stop there. Worst case: a call started just before the deadline runs its
//     10 s timeout, which leaves 10 s of the 20 s margin for an abandoned R2 or D1 write to land and for clocks;
//   - the commit (moving LATEST, or the withdraw and index status change) is a transaction with a check that the lock
//     still names this request as its owner, so a holder that lost its lock can never change the live record.
import { randomHex } from "@news/worker-shared/crypto";
import { type Cond, eq, lt, notExists, or } from "@news/worker-shared/dynamo/conditions";
import {
  type BatchWriteOptions,
  ConditionFailedError,
  type Doc,
  type Key,
  OutcomeUnknownError,
  type Table,
} from "@news/worker-shared/dynamo/table";
import { pageLockKey } from "@news/worker-shared/keys";
import type { PageLock } from "@news/worker-shared/records";
import { ApiError } from "./errors";

export const LEASE_MS = 2 * 60 * 1000;
/** No page write starts later than this after the lock was taken: 20 s of margin for one call's timeout and clocks. */
export const DEADLINE_MS = LEASE_MS - 20_000;
/** Default timeout of one R2 or D1 call. */
export const STORAGE_TIMEOUT_MS = 10_000;

export interface HeldLock {
  key: Key;
  owner: string;
  /** When the lock was taken (epoch ms): the deadline counts from here. */
  acquiredAt: number;
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
  return { key, owner, acquiredAt: now.getTime() };
}

/** Gives the lock back (only our own). Best effort: a lock that stays behind goes stale after LEASE_MS. */
export async function unlockPage(table: Table, lock: HeldLock): Promise<void> {
  await table.delete(lock.key, eq("owner", lock.owner)).catch(() => {});
}

const lockExpired = () =>
  new ApiError(
    "lock_expired",
    "the operation ran out of time on its page lock; retry with the same Idempotency-Key",
    undefined,
    { "retry-after": "5" },
  );

/** Keeps a page operation inside its lock. Without a lock (other routes, direct calls in tests) only timeouts apply. */
export class Fence {
  constructor(
    readonly lock: HeldLock | undefined,
    readonly now: () => Date,
    readonly timeoutMs: number = STORAGE_TIMEOUT_MS,
  ) {}

  /** Epoch ms after which no page write starts; undefined without a lock. */
  get deadline(): number | undefined {
    return this.lock && this.lock.acquiredAt + DEADLINE_MS;
  }

  /** Throws lock_expired once past the deadline. Called before every step and every batch of page writes. */
  check(): void {
    const deadline = this.deadline;
    if (deadline !== undefined && this.now().getTime() > deadline) throw lockExpired();
  }

  /**
   * Options for Table.batchWrite: the deadline on this fence's clock, so a retry of unprocessed writes never starts
   * after it either (a retry started near the deadline could otherwise land after the lease).
   */
  batchOptions(): BatchWriteOptions {
    const deadline = this.deadline;
    return deadline === undefined ? {} : { deadline, now: () => this.now().getTime() };
  }

  /**
   * An R2 or D1 call with a timeout. A call that times out may still be applied later, so it fails as an unknown
   * outcome: the request then keeps its lock until it is stale, and no one else writes the page meanwhile.
   */
  bounded<T>(what: string, call: Promise<T>): Promise<T> {
    let timer: ReturnType<typeof setTimeout> | undefined;
    const timeout = new Promise<never>((_, reject) => {
      timer = setTimeout(() => reject(new OutcomeUnknownError(`${what} timed out`)), this.timeoutMs);
    });
    return Promise.race([call, timeout]).finally(() => clearTimeout(timer));
  }

  /**
   * A conditional put that also requires the lock to still be ours (one transaction), with `also` (unconditional
   * puts, such as the request's result record) in the same transaction. Throws lock_expired when the lock is not
   * ours, and ConditionFailedError when only `cond` failed.
   */
  async put(table: Table, item: Doc, cond: Cond, also: Doc[] = []): Promise<void> {
    this.check();
    const lock = this.lock;
    const ops = [{ put: item, cond }, ...also.map((put) => ({ put }))];
    if (!lock) return also.length > 0 ? table.transact(ops) : table.put(item, cond);
    try {
      await table.transact([{ check: lock.key, cond: eq("owner", lock.owner) }, ...ops]);
    } catch (e) {
      if (e instanceof ConditionFailedError && (await table.get(lock.key))?.owner !== lock.owner) throw lockExpired();
      throw e;
    }
  }
}
