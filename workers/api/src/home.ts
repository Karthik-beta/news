// Home curation: PUT /v1/home/curation (validate again, write V#<n>, move LATEST, purge home and region views)
// and POST /v1/home/curation/rollback (make an earlier version live). Nothing is rebuilt or redeployed: the SSR
// Worker reads LATEST at request time (TD "Home page curation").
import { checkCuration, type PageLookup, slotsOf } from "@news/worker-shared/curation";
import { eq, notExists } from "@news/worker-shared/dynamo/conditions";
import { ConditionFailedError } from "@news/worker-shared/dynamo/table";
import { curationLatestKey, curationVersionKey } from "@news/worker-shared/keys";
import { Reader } from "@news/worker-shared/read";
import type { Curation, CurationRecord } from "@news/worker-shared/records";
import { homeUrls } from "@news/worker-shared/urls";
import { check, schemas } from "@news/worker-shared/validate";
import type { OpContext } from "./app";
import type { Deps } from "./deps";
import { ApiError } from "./errors";
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
  curation: Curation;
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
 * Writes the next version and makes it live in one transaction, then purges. A retry of the same request (same
 * Idempotency-Key, after a failed purge, a lost response or a crash) finds the version it already wrote, by its
 * request_id, and only purges again: one curation never becomes two versions.
 */
export async function executeCuration(deps: Deps, p: PreparedCuration, ctx: OpContext): Promise<OpResult> {
  const table = deps.table();
  const reader = new Reader(table);
  const now = deps.now().toISOString();
  const [recent, live] = await Promise.all([reader.recentCurationVersions(), reader.getLiveCuration()]);
  const done = recent.find((r) => r.request_id === ctx.requestId);
  if (done) {
    const purged = await deps.purger.purge(homeUrls(deps.siteOrigin, deps.regions));
    return { status: 200, body: { version: done.version, live_version: live?.version ?? done.version, purged } };
  }
  const version = (recent[0]?.version ?? 0) + 1;
  const record: CurationRecord = {
    ...p.curation,
    ...curationVersionKey(version),
    PK: "HOME#CURATION",
    type: "curation",
    version,
    published_at: now,
    server_checks: { passed: true, run_at: now },
    request_id: ctx.requestId,
  };
  try {
    // Both or neither: a new immutable version, and LATEST moved to it (only if nobody moved it meanwhile).
    await table.transact([
      { put: record as never, cond: notExists() },
      {
        put: { ...record, ...curationLatestKey(), PK: "HOME#CURATION", made_live_at: now } as never,
        cond: live ? eq("version", live.version) : notExists(),
      },
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
