// Home curation: PUT /v1/home/curation (validate again, write V#<n>, move LATEST, purge home and region views)
// and POST /v1/home/curation/rollback (make an earlier version live). Nothing is rebuilt or redeployed: the SSR
// Worker reads LATEST at request time (TD "Home page curation").
import { checkCuration, type PageLookup, slotsOf } from "@news/worker-shared/curation";
import { eq, notExists } from "@news/worker-shared/dynamo/conditions";
import { ConditionFailedError } from "@news/worker-shared/dynamo/table";
import { curationLatestKey, curationVersionKey, operationResultKey } from "@news/worker-shared/keys";
import { Reader } from "@news/worker-shared/read";
import type { Curation, CurationRecord, OperationResult } from "@news/worker-shared/records";
import { homeUrls } from "@news/worker-shared/urls";
import { check, schemas } from "@news/worker-shared/validate";
import type { OpContext } from "./app";
import type { Deps } from "./deps";
import { ApiError } from "./errors";
import { TTL_SECONDS } from "./idempotency";
import type { OpResult } from "./pages";

/** Looks up every curated page: live record, or missing, or versions without a live record ("unpublished"). */
export async function lookupPages(reader: Reader, ids: string[]): Promise<Map<string, PageLookup>> {
  const live = await reader.getPages(ids);
  const out = new Map<string, PageLookup>();
  for (const id of new Set(ids)) {
    const page = live.get(id);
    if (page) out.set(id, { state: "live", page });
    else out.set(id, (await reader.hasAnyVersion(id)) ? { state: "unpublished" } : { state: "missing" });
  }
  return out;
}

export interface PreparedCuration {
  curation?: Curation;
  /** The version this request already committed (found by its key): only the purge is left to do. */
  committed?: number;
}

/**
 * Before any validation: the version this request (same key, same body) already committed, if any. The publish
 * transaction records it under the key (IDEM#<key hash> / RESULT), so a retry after a failed purge, a lost response
 * or a crash only purges again, however many curations were published since and whatever changed in the pages.
 */
export async function recoverCuration(
  deps: Deps,
  seen: { keyHash: string; requestHash: string },
): Promise<PreparedCuration | undefined> {
  const done = (await deps.table().get(operationResultKey(seen.keyHash))) as unknown as OperationResult | undefined;
  return done && done.request_hash === seen.requestHash ? { committed: done.version } : undefined;
}

export async function prepareCuration(deps: Deps, body: unknown): Promise<PreparedCuration> {
  const c = check<Curation>(schemas.validateCuration, body);
  if (!c.ok) throw new ApiError("invalid_curation", "the curation does not match curation.v1", c.issues);
  const reader = new Reader(deps.table());
  const pages = await lookupPages(
    reader,
    slotsOf(c.value).map((s) => s.slot.page_id),
  );
  const issues = checkCuration(c.value, pages, deps.now());
  if (issues.length > 0) throw new ApiError("invalid_curation", "the curation failed the home checks", issues);
  return { curation: c.value };
}

/**
 * Writes the next version, makes it live and records it under the request's key, in one transaction, then purges.
 * A retry of the same request finds that record first (recoverCuration) and only purges again, answering as the
 * first attempt did: one curation never becomes two versions.
 */
export async function executeCuration(deps: Deps, p: PreparedCuration, ctx: OpContext): Promise<OpResult> {
  if (p.committed !== undefined) {
    const purged = await deps.purger.purge(homeUrls(deps.siteOrigin, deps.regions));
    return { status: 200, body: { version: p.committed, live_version: p.committed, purged } };
  }
  const table = deps.table();
  const reader = new Reader(table);
  const at = deps.now();
  const now = at.toISOString();
  const [newest, live] = await Promise.all([reader.newestCurationVersion(), reader.getLiveCuration()]);
  const version = newest + 1;
  const record: CurationRecord = {
    ...(p.curation as Curation),
    ...curationVersionKey(version),
    PK: "HOME#CURATION",
    type: "curation",
    version,
    published_at: now,
    server_checks: { passed: true, run_at: now },
    request_id: ctx.requestId,
  };
  const result: OperationResult = {
    ...operationResultKey(ctx.requestId),
    SK: "RESULT",
    type: "operation_result",
    operation: "home_curation",
    request_hash: ctx.requestHash,
    version,
    created_at: now,
    expires_at: Math.floor(at.getTime() / 1000) + TTL_SECONDS,
  };
  try {
    // Both or neither: a new immutable version, and LATEST moved to it (only if nobody moved it meanwhile).
    await table.transact([
      { put: record as never, cond: notExists() },
      {
        put: { ...record, ...curationLatestKey(), PK: "HOME#CURATION", made_live_at: now } as never,
        cond: live ? eq("version", live.version) : notExists(),
      },
      { put: result as never },
    ]);
  } catch (e) {
    if (e instanceof ConditionFailedError)
      throw new ApiError("version_conflict", "another curation was published at the same time; retry");
    throw e;
  }
  const purged = await deps.purger.purge(homeUrls(deps.siteOrigin, deps.regions));
  return { status: 200, body: { version, live_version: version, purged } };
}

export interface PreparedRollback {
  target: CurationRecord;
  live: CurationRecord | undefined;
  newest: number;
}

export async function prepareRollback(deps: Deps, body: unknown): Promise<PreparedRollback> {
  const r = check<{ version: number }>(schemas.validateRollbackRequest, body);
  if (!r.ok) throw new ApiError("invalid_request", "the request body does not match rollbackRequest", r.issues);
  const reader = new Reader(deps.table());
  const [target, live, newest] = await Promise.all([
    reader.getCurationVersion(r.value.version),
    reader.getLiveCuration(),
    reader.newestCurationVersion(),
  ]);
  if (!target) throw new ApiError("not_found", `curation version ${r.value.version} does not exist`);
  return { target, live, newest };
}

export async function executeRollback(deps: Deps, p: PreparedRollback): Promise<OpResult> {
  const now = deps.now().toISOString();
  try {
    await deps
      .table()
      .put(
        { ...p.target, ...curationLatestKey(), PK: "HOME#CURATION", made_live_at: now } as never,
        p.live ? eq("version", p.live.version) : notExists(),
      );
  } catch (e) {
    if (e instanceof ConditionFailedError)
      throw new ApiError("version_conflict", "the live curation changed while rolling back; retry");
    throw e;
  }
  const purged = await deps.purger.purge(homeUrls(deps.siteOrigin, deps.regions));
  return { status: 200, body: { version: p.newest, live_version: p.target.version, purged } };
}
