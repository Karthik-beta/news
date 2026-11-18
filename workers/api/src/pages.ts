// Page operations: publish (PUT /v1/pages/{id}), withdraw, and index promotion.
import { sha256Hex } from "@news/worker-shared/crypto";
import { and, eq, le, notExists, or } from "@news/worker-shared/dynamo/conditions";
import {
  BATCH_WRITE_MAX,
  type BatchOp,
  ConditionFailedError,
  type Doc,
  itemSize,
  OutcomeUnknownError,
  type Table,
} from "@news/worker-shared/dynamo/table";
import {
  entityKey,
  hubPk,
  hubSk,
  hubsForPage,
  pageLatestKey,
  pageVersionKey,
  pathClaimKey,
} from "@news/worker-shared/keys";
import { PURGE_BATCH } from "@news/worker-shared/purge";
import { deletePageFiles, putPageFile } from "@news/worker-shared/r2";
import { Reader } from "@news/worker-shared/read";
import type {
  EntityRecord,
  HubEntry,
  Manifest,
  OperationResult,
  PageMeta,
  PageRecord,
  PathClaim,
} from "@news/worker-shared/records";
import { MissingShardError } from "@news/worker-shared/search";
import { normalizeTimestamp } from "@news/worker-shared/time";
import { absolute, HOME_PATH, hubPath, pageFileUrls } from "@news/worker-shared/urls";
import { check, type Issue, schemas } from "@news/worker-shared/validate";
import type { OpContext } from "./app";
import type { Deps } from "./deps";
import { ApiError } from "./errors";
import { committedResult, resultItem } from "./idempotency";
import { Fence } from "./lock";

export const REQUIRED_FILES = ["index.html", "index.md", "data.json", "sources.json", "meta.json", "page.oui"];
/** Page record items stay far below DynamoDB's 400 KB: metadata only, files in R2 (ADR 0004). */
const MAX_PAGE_ITEM_BYTES = 64 * 1024;
const CONCURRENCY = 6;

/**
 * External subrequests one publish may plan. The Workers free plan allows 50 per request, and every DynamoDB call and
 * purge call is one (R2 and D1 bindings count against Cloudflare's separate, larger limit for its own services). The
 * five left over cover retries of unprocessed batch writes and one rate-limited purge batch.
 */
export const SUBREQUEST_BUDGET = 45;
/**
 * DynamoDB calls of a publish that are not batches: idempotency lookup, claim and completion; page lock and unlock;
 * live record, newer version records and stored version reads; path claim; version record; LATEST; clearing the
 * cleanup note.
 */
const PUBLISH_SINGLE_CALLS = 12;
const batches = (n: number, size: number) => Math.ceil(n / size);

export interface OpResult {
  status: number;
  body: unknown;
}

/**
 * Runs `fn` over `items` with at most `n` in flight (keeps DynamoDB bursts small). After a failure nothing new is
 * started, but every call already running is waited for before the failure is thrown: the caller gives the page lock
 * back only once no write of this request can still land. An outcome-unknown failure wins over any other.
 */
export async function eachLimited<T>(items: T[], n: number, fn: (item: T) => Promise<unknown>): Promise<void> {
  let next = 0;
  const errors: unknown[] = [];
  const worker = async () => {
    while (next < items.length && errors.length === 0) {
      try {
        await fn(items[next++] as T);
      } catch (e) {
        errors.push(e);
      }
    }
  };
  await Promise.all(Array.from({ length: Math.min(n, items.length) }, worker));
  if (errors.length > 0) throw errors.find((e) => e instanceof OutcomeUnknownError) ?? errors[0];
}

// --- publish ------------------------------------------------------------------------------------------------------

export interface CheckedBundle {
  meta: PageMeta;
  manifest: Manifest;
  files: Record<string, string>;
}

export interface PreparedPage extends CheckedBundle {
  previous: PageRecord | undefined;
  /** The new live record, its cleanup note, and the entity records to write: planned before anything is written. */
  page: PageRecord;
  cleanup: PageRecord["cleanup"];
  entities: EntityRecord[];
}

const enc = new TextEncoder();

/** Validates the whole bundle (no reads, no writes): schema, hashes and sizes, meta. */
export async function checkBundle(pageId: string, body: unknown): Promise<CheckedBundle> {
  const req = check<{ manifest: Manifest; files: Record<string, string> }>(schemas.validatePutPageRequest, body);
  if (!req.ok) throw new ApiError("invalid_request", "the request body does not match putPageRequest", req.issues);
  const { manifest, files } = req.value;
  const issues: Issue[] = [];
  const bad = (code: string, message: string, path?: string) =>
    issues.push({ code, message, ...(path ? { path } : {}) });

  if (manifest.page_id !== pageId) bad("bundle.page_id", "manifest.page_id differs from the URL", "/manifest/page_id");
  const listed = new Map(manifest.files.map((f) => [f.path, f]));
  if (listed.size !== manifest.files.length) bad("bundle.duplicate_file", "a file is listed twice", "/manifest/files");
  for (const name of REQUIRED_FILES)
    if (!listed.has(name)) bad("bundle.missing_file", `${name} is missing`, "/manifest/files");
  for (const name of Object.keys(files))
    if (!listed.has(name)) bad("bundle.unlisted_file", `${name} is not in the manifest`, `/files/${name}`);
  for (const f of manifest.files) {
    const text = files[f.path];
    if (text === undefined) {
      bad("bundle.missing_content", `no content for ${f.path}`, `/files/${f.path}`);
      continue;
    }
    if (enc.encode(text).length !== f.bytes)
      bad("bundle.size_mismatch", `${f.path}: size differs from the manifest`, `/files/${f.path}`);
    else if ((await sha256Hex(text)) !== f.sha256)
      bad("bundle.hash_mismatch", `${f.path}: SHA-256 differs from the manifest`, `/files/${f.path}`);
  }
  const sorted = [...manifest.files].sort((a, b) => (a.path < b.path ? -1 : a.path > b.path ? 1 : 0));
  if ((await sha256Hex(sorted.map((f) => `${f.path} ${f.sha256}\n`).join(""))) !== manifest.bundle_sha256)
    bad("bundle.bundle_sha256", "bundle_sha256 does not match the file list", "/manifest/bundle_sha256");
  for (const name of ["data.json", "sources.json"]) {
    const text = files[name];
    if (text === undefined) continue;
    try {
      JSON.parse(text);
    } catch {
      bad("bundle.invalid_json", `${name} is not valid JSON`, `/files/${name}`);
    }
  }
  if (issues.length > 0) throw new ApiError("invalid_bundle", "the bundle is not consistent", issues);

  let metaJson: unknown;
  try {
    metaJson = JSON.parse(files["meta.json"] as string);
  } catch {
    throw new ApiError("invalid_meta", "meta.json is not valid JSON");
  }
  const m = check<PageMeta>(schemas.validateMeta, metaJson);
  if (!m.ok) throw new ApiError("invalid_meta", "meta.json does not match meta.v1", m.issues);
  // Timestamps are stored in one form (toISOString), so they compare and sort as times; a day that does not exist
  // (2026-02-31) is refused. meta.json itself stays byte for byte as sent.
  const publishedAt = normalizeTimestamp(m.value.published_at);
  const updatedAt = normalizeTimestamp(m.value.updated_at);
  if (!publishedAt) bad("meta.published_at", "published_at is not a real UTC time", "/published_at");
  if (!updatedAt) bad("meta.updated_at", "updated_at is not a real UTC time", "/updated_at");
  const meta: PageMeta = {
    ...m.value,
    published_at: publishedAt ?? m.value.published_at,
    updated_at: updatedAt ?? m.value.updated_at,
  };
  if (meta.id !== pageId) bad("meta.id", "meta.id differs from the URL", "/id");
  if (meta.version !== manifest.version) bad("meta.version", "meta.version differs from manifest.version", "/version");
  if (publishedAt && updatedAt && updatedAt < publishedAt)
    bad("meta.updated_at", "updated_at is before published_at", "/updated_at");
  for (const name of Object.keys(files)) {
    const chart = /^charts\/(.+)\.csv$/.exec(name)?.[1];
    if (chart && !meta.chart_ids.includes(chart))
      bad("meta.chart_ids", `charts/${chart}.csv is not in chart_ids`, "/chart_ids");
  }
  if (issues.length > 0) throw new ApiError("invalid_meta", "meta.json does not fit the bundle", issues);
  return { meta, manifest, files };
}

/**
 * Checks (no writes, page lock held) that the bundle may replace the live version and that its shard is bound, and
 * plans every write: the new record, what to clean up, and the subrequests it all takes.
 */
export async function preparePut(deps: Deps, pageId: string, bundle: CheckedBundle): Promise<PreparedPage> {
  const { meta, manifest } = bundle;
  const reader = new Reader(deps.table());
  const previous = await reader.getPage(pageId);
  // Versions above the live one never committed. Their version records list what they may have written (files, hub
  // entries), so this publish removes whatever of that it does not keep, and keeps their URL and publication time.
  const newer = await reader.pageVersionsAfter(pageId, previous?.version ?? 0);
  const firstTry = newer[0];
  // A page keeps its first publication time, and with it its search shard: check the shard the record will use.
  try {
    deps.search.assertShard(previous?.published_at ?? firstTry?.published_at ?? meta.published_at);
  } catch (e) {
    if (e instanceof MissingShardError) {
      deps.log.error("search_shard_missing", { year: e.year });
      throw new ApiError("not_configured", `no search shard is bound for ${e.year}`);
    }
    throw e;
  }

  if (previous) assertCanReplace(previous, meta, manifest);
  else if (firstTry && firstTry.path !== meta.path)
    throw new ApiError(
      "path_conflict",
      `page ${meta.id} was first published at ${firstTry.path}; a page never changes its URL`,
    );
  // A version number is used once. A stored version with other content (one that never went live) is refused here,
  // before any file is written.
  const stored = newer.find((v) => v.version === meta.version) ?? (await reader.getPageVersion(pageId, meta.version));
  if (stored && stored.bundle_sha256 !== manifest.bundle_sha256)
    throw new ApiError("version_conflict", `version ${meta.version} already exists with different content`);

  const now = deps.now();
  const page = pageRecord(meta, manifest, previous, now, firstTry);
  const cleanup = cleanupFor(
    previous,
    page,
    newer.filter((v) => v.version !== page.version),
  );
  const entities = newEntities(meta, previous, page.lang, now);
  const planned =
    PUBLISH_SINGLE_CALLS +
    batches(page.hubs.length + entities.length, BATCH_WRITE_MAX) +
    batches(cleanup?.hubs.length ?? 0, BATCH_WRITE_MAX) +
    batches(new Set(affectedUrls(deps, { ...page, cleanup })).size, PURGE_BATCH);
  if (planned > SUBREQUEST_BUDGET)
    throw new ApiError("invalid_meta", "this publish needs more storage calls than one request may make", [
      {
        code: "meta.too_many_writes",
        message: `${planned} DynamoDB and purge calls planned; at most ${SUBREQUEST_BUDGET} fit in one request`,
      },
    ]);
  return { ...bundle, previous, page, cleanup, entities };
}

/** Entity records to write: those the live version does not already list with the same name and kind. */
function newEntities(meta: PageMeta, previous: PageRecord | undefined, lang: string, now: Date): EntityRecord[] {
  const entityId = (e: { id: string; name: string; kind: string }) => `${e.id}\u0000${e.name}\u0000${e.kind}`;
  const known = new Set((previous?.entities ?? []).map(entityId));
  const byId = new Map<string, EntityRecord>();
  for (const e of meta.entities)
    if (!known.has(entityId(e)))
      byId.set(e.id, {
        ...entityKey(e.id),
        SK: "META",
        type: "entity",
        lang,
        id: e.id,
        name: e.name,
        kind: e.kind,
        updated_at: now.toISOString(),
      });
  return [...byId.values()];
}

/**
 * Unconditional writes in batches of 25 (one subrequest per batch), each started only before the lock's deadline; the
 * table gets the deadline too, so no retry of unprocessed writes starts after it.
 */
async function writeBatches(table: Table, fence: Fence, ops: BatchOp[]): Promise<void> {
  for (let i = 0; i < ops.length; i += BATCH_WRITE_MAX) {
    fence.check();
    await table.batchWrite(ops.slice(i, i + BATCH_WRITE_MAX), fence.batchOptions());
  }
}

const hubKey = (page: Pick<PageRecord, "published_at" | "page_id">, hub: string) => ({
  PK: hubPk(hub),
  SK: hubSk(page.published_at, page.page_id),
});

function assertCanReplace(previous: PageRecord, meta: PageMeta, manifest: Manifest): void {
  if (previous.path !== meta.path)
    throw new ApiError(
      "path_conflict",
      `page ${meta.id} is published at ${previous.path}; a page never changes its URL`,
    );
  // Withdrawn stays withdrawn: only a higher version brings the page back (a retry of the old one must not).
  if (previous.status === "withdrawn" && meta.version <= previous.version)
    throw new ApiError(
      "page_withdrawn",
      `page ${meta.id} was withdrawn at version ${previous.version}; publish a higher version to bring it back`,
    );
  if (meta.version < previous.version)
    throw new ApiError(
      "version_conflict",
      `version ${meta.version} is older than the live version ${previous.version}`,
    );
  if (meta.version === previous.version && manifest.bundle_sha256 !== previous.bundle_sha256)
    throw new ApiError("version_conflict", `version ${meta.version} is already published with different content`);
}

export function pageRecord(
  meta: PageMeta,
  manifest: Manifest,
  previous: PageRecord | undefined,
  now: Date,
  /** The oldest version record above the live one, when a first attempt never committed. */
  firstTry?: Pick<PageRecord, "published_at">,
): PageRecord {
  const { schema: _schema, id, ...fields } = meta;
  // A page keeps its first publication time, so its hub entries keep their place and SK.
  const publishedAt = previous?.published_at ?? firstTry?.published_at ?? meta.published_at;
  return {
    ...pageLatestKey(id),
    type: "page",
    ...fields,
    page_id: id,
    published_at: publishedAt,
    // After the first publication index_status belongs to POST /v1/pages/{id}/index: a new version keeps the live
    // value, so republishing never silently undoes a promotion (or a demotion).
    index_status: previous?.index_status ?? meta.index_status,
    status: "published",
    bundle_sha256: manifest.bundle_sha256,
    files: manifest.files.map((f) => f.path).sort(),
    hubs: hubsForPage(meta),
    written_at: now.toISOString(),
  };
}

export function hubEntry(page: PageRecord, hub: string): HubEntry {
  return {
    PK: hubPk(hub),
    SK: hubSk(page.published_at, page.page_id),
    type: "hub_entry",
    lang: page.lang,
    hub,
    page_id: page.page_id,
    path: page.path,
    title: page.title,
    ...(page.fly_title ? { fly_title: page.fly_title } : {}),
    summary: page.summary,
    page_type: page.page_type,
    tier: page.tier,
    region: page.region,
    published_at: page.published_at,
    updated_at: page.updated_at,
    index_status: page.index_status,
  };
}

const unique = (...lists: string[][]) => [...new Set(lists.flat())];

/**
 * What the replaced version had and this one dropped: hubs to leave and files to delete. A retry of the live version
 * keeps the cleanup it recorded; a new version also takes over whatever the live one recorded and has not finished,
 * and whatever versions that never committed (`abandoned`, from their version records) may have written.
 */
export function cleanupFor(
  previous: PageRecord | undefined,
  page: PageRecord,
  abandoned: Pick<PageRecord, "hubs" | "files">[] = [],
): PageRecord["cleanup"] {
  const retry =
    previous?.status === "published" &&
    previous.version === page.version &&
    previous.bundle_sha256 === page.bundle_sha256;
  const replaced = previous && !retry ? [previous] : [];
  const hubs = unique(
    ...replaced.map((v) => v.hubs),
    previous?.cleanup?.hubs ?? [],
    ...abandoned.map((v) => v.hubs),
  ).filter((h) => !page.hubs.includes(h));
  const files = unique(
    ...replaced.map((v) => v.files),
    previous?.cleanup?.files ?? [],
    ...abandoned.map((v) => v.files),
  ).filter((f) => !page.files.includes(f));
  return hubs.length + files.length > 0 ? { hubs, files } : undefined;
}

/** URLs to purge after a page changes: its files, every hub it is or was in, and the home page. */
function affectedUrls(deps: Deps, page: Pick<PageRecord, "path" | "hubs" | "files" | "cleanup">): string[] {
  const hubs = unique(page.hubs, page.cleanup?.hubs ?? []);
  const files = unique(page.files, page.cleanup?.files ?? []);
  return [...pageFileUrls(deps.siteOrigin, page.path, files), ...listUrls(deps, hubs)];
}

/** The pages that list pages: the given hubs and the home page. */
const listUrls = (deps: Deps, hubs: string[]) => [
  ...hubs.map((h) => absolute(deps.siteOrigin, hubPath(h))),
  absolute(deps.siteOrigin, HOME_PATH),
];

/** The context of a direct call (tests): no page lock, so only the storage timeouts apply. */
const unlocked = (deps: Deps): OpContext => ({
  requestId: "",
  requestHash: "",
  fence: new Fence(undefined, deps.now, deps.storageTimeoutMs),
});

export async function executePut(deps: Deps, p: PreparedPage, ctx: OpContext = unlocked(deps)): Promise<OpResult> {
  const table = deps.table();
  const bucket = deps.bucket();
  const { fence } = ctx;
  const { page, cleanup } = p;
  const live: PageRecord = cleanup ? { ...page, cleanup } : page;
  if (itemSize(live as unknown as Doc) > MAX_PAGE_ITEM_BYTES)
    throw new ApiError("invalid_meta", "page metadata is too large for one record (64 KB)");

  // The caller holds the page lock (lock.ts) and preparePut refused every conflict it could see, so nothing below
  // races another write to this page. Every step is idempotent: a retry after a failure anywhere finishes. The fence
  // stops the work at the lock's deadline, bounds every R2 and D1 call, and makes the commit check the lock.

  // 1. Claim the path for this page, so no other page can publish at the same URL (or R2 prefix).
  const claimItem: PathClaim = {
    ...pathClaimKey(page.path),
    SK: "PAGE",
    type: "path_claim",
    lang: page.lang,
    page_id: page.page_id,
    path: page.path,
    created_at: page.written_at,
  };
  fence.check();
  try {
    await table.put(claimItem as never, or(notExists(), eq("page_id", page.page_id)));
  } catch (e) {
    if (e instanceof ConditionFailedError) throw new ApiError("path_conflict", `${page.path} belongs to another page`);
    throw e;
  }

  // 2. The immutable version record, before anything it lists is written: if this publish never commits, the next one
  //    (or a withdrawal) finds it above LATEST and removes its files and hub entries. An identical retry passes,
  //    different content under the same version does not.
  fence.check();
  try {
    await table.put(
      { ...page, SK: pageVersionKey(page.page_id, page.version).SK } as never,
      or(notExists(), eq("bundle_sha256", page.bundle_sha256)),
    );
  } catch (e) {
    if (e instanceof ConditionFailedError)
      throw new ApiError("version_conflict", `version ${page.version} already exists with different content`);
    throw e;
  }

  // 3. The files, each stamped with its version, so a reader can tell which version it got.
  const stamp = { version: page.version, bundle_sha256: page.bundle_sha256 };
  await eachLimited(Object.entries(p.files), CONCURRENCY, ([name, text]) => {
    fence.check();
    return fence.bounded(`R2 put ${name}`, putPageFile(bucket, page.path, name, text, stamp));
  });

  // 4. Hub entries and entities, in batches. An entity the live version already listed with the same name and kind
  //    is not rewritten: its item exists, because entities are written before LATEST moves.
  await writeBatches(table, fence, [
    ...page.hubs.map((h) => ({ put: hubEntry(page, h) as unknown as Doc })),
    ...p.entities.map((e) => ({ put: e as unknown as Doc })),
  ]);

  // 5. Search row, then LATEST last: moving LATEST is the commit point readers see. It is one transaction with a
  //    check that this request still holds the page lock.
  fence.check();
  await fence.bounded("search index", deps.search.index(page));
  try {
    await fence.put(table, live as unknown as Doc, or(notExists(), le("version", page.version)));
  } catch (e) {
    if (e instanceof ConditionFailedError)
      throw new ApiError("version_conflict", "a newer version was published while this one was being written");
    throw e;
  }

  // 6. After the commit: leave the hubs and delete the files the page dropped, purge, then clear the cleanup note.
  //    The note is part of LATEST, so a retry of this request (or the next version) finishes it if this fails.
  if (cleanup) {
    await writeBatches(
      table,
      fence,
      cleanup.hubs.map((h) => ({ delete: hubKey(page, h) })),
    );
    fence.check();
    await fence.bounded("R2 delete", deletePageFiles(bucket, page.path, cleanup.files));
  }
  fence.check();
  const purged = await deps.purger.purge(affectedUrls(deps, live));
  if (cleanup) {
    await fence
      .put(
        table,
        page as unknown as Doc,
        and(eq("version", page.version), eq("bundle_sha256", page.bundle_sha256), eq("status", "published")),
      )
      .catch((e) => {
        if (!(e instanceof ConditionFailedError)) throw e;
      });
  }
  return {
    status: 200,
    body: {
      page_id: page.page_id,
      version: page.version,
      url: absolute(deps.siteOrigin, page.path),
      status: "published",
      purged,
    },
  };
}

// --- withdraw and index promotion ---------------------------------------------------------------------------------

export interface PreparedPageOp {
  page: PageRecord;
  reason?: string;
  indexStatus?: "index" | "noindex";
  /** Withdraw: hubs and files listed by version records above the live one (publishes that never committed). */
  leftovers?: { hubs: string[]; files: string[] };
}

export async function checkWithdraw(body: unknown): Promise<{ reason: string }> {
  const req = check<{ reason: string }>(schemas.validateWithdrawRequest, body);
  if (!req.ok) throw new ApiError("invalid_request", "the request body does not match withdrawRequest", req.issues);
  return req.value;
}

export type PreparedWithdraw =
  | (PreparedPageOp & {
      /** No LATEST yet (a first publish that never committed): `page` is its newest version record. */
      neverLive?: boolean;
      committed?: undefined;
    })
  | { committed: OperationResult };

/** Before any check: the withdrawal this request (same key and body) already committed, if any. */
export async function recoverWithdraw(
  deps: Deps,
  seen: { keyHash: string; requestHash: string },
): Promise<PreparedWithdraw | undefined> {
  const committed = await committedResult(deps.table(), seen, "withdraw");
  return committed ? { committed } : undefined;
}

export async function prepareWithdraw(
  deps: Deps,
  pageId: string,
  req: { reason: string },
): Promise<PreparedPageOp & { neverLive?: boolean }> {
  const reader = new Reader(deps.table());
  const live = await reader.getPage(pageId);
  const newer = await reader.pageVersionsAfter(pageId, live?.version ?? 0);
  // Without LATEST, a first publish may still have written files and hub entries: withdraw its newest attempt.
  const newest = newer[newer.length - 1];
  const page = live ?? (newest && { ...newest, ...pageLatestKey(pageId) });
  if (!page) throw new ApiError("not_found", `page ${pageId} does not exist`);
  const leftovers = { hubs: unique(...newer.map((v) => v.hubs)), files: unique(...newer.map((v) => v.files)) };
  return { page, reason: req.reason, leftovers, ...(live ? {} : { neverLive: true }) };
}

/**
 * Marks the page withdrawn (unless it already is) and records that under the request's key in the same transaction,
 * then removes its search row and public files, purges, and removes its hub entries (and those of any publish that
 * never committed). Those steps run on every attempt, including when the page is already withdrawn, so a retry
 * finishes a withdrawal that failed half way. A retry of a request that committed while a later version is live only
 * purges again and answers as the first attempt did. Versions stay in DynamoDB and the bundle on the laptop.
 */
export async function executeWithdraw(
  deps: Deps,
  prepared: PreparedWithdraw,
  ctx: OpContext = unlocked(deps),
): Promise<OpResult> {
  const table = deps.table();
  const { fence } = ctx;
  let p: PreparedPageOp & { neverLive?: boolean };
  if (prepared.committed) {
    const done = prepared.committed;
    const again = await prepareWithdraw(deps, done.page_id as string, { reason: "" });
    if (again.page.status !== "withdrawn" || again.page.version !== done.version) return replayWithdraw(deps, done);
    p = again;
  } else p = prepared;
  let page = p.page;
  if (page.status !== "withdrawn") {
    const now = deps.now();
    const withdrawn: PageRecord = {
      ...page,
      status: "withdrawn",
      withdrawn_at: now.toISOString(),
      withdraw_reason: p.reason,
    };
    const recorded = { version: page.version, page_id: page.page_id, index_status: page.index_status };
    try {
      await fence.put(
        table,
        withdrawn as unknown as Doc,
        p.neverLive ? notExists() : and(eq("version", page.version), eq("status", "published")),
        ctx.requestId ? [resultItem(ctx, "withdraw", now, recorded) as unknown as Doc] : [],
      );
    } catch (e) {
      if (e instanceof ConditionFailedError)
        throw new ApiError("version_conflict", "the page changed; read it again and retry");
      throw e;
    }
    page = withdrawn;
  } else if (ctx.requestId && !prepared.committed) {
    // Already withdrawn (by another request): record this request's result as well, so a retry of it after a later
    // version went live finds it and only purges, instead of withdrawing that version. The withdrawn LATEST is
    // rewritten unchanged, which checks in the same transaction that it is still this withdrawn version.
    try {
      await fence.put(table, page as unknown as Doc, and(eq("version", page.version), eq("status", "withdrawn")), [
        resultItem(ctx, "withdraw", deps.now(), {
          version: page.version,
          page_id: page.page_id,
          index_status: page.index_status,
        }) as unknown as Doc,
      ]);
    } catch (e) {
      if (e instanceof ConditionFailedError)
        throw new ApiError("version_conflict", "the page changed; read it again and retry");
      throw e;
    }
  }
  const gone = page;
  const removed = {
    hubs: unique(gone.hubs, gone.cleanup?.hubs ?? [], p.leftovers?.hubs ?? []),
    files: unique(gone.files, gone.cleanup?.files ?? [], p.leftovers?.files ?? []),
  };
  fence.check();
  await fence.bounded("search remove", deps.search.remove(gone));
  // The public files go first and are purged at once: a withdrawn page must stop being served, even if a hub batch
  // fails below.
  fence.check();
  await fence.bounded("R2 delete", deletePageFiles(deps.bucket(), gone.path, removed.files));
  fence.check();
  const purgedFiles = await deps.purger.purge(pageFileUrls(deps.siteOrigin, gone.path, removed.files));
  await writeBatches(
    table,
    fence,
    removed.hubs.map((h) => ({ delete: hubKey(gone, h) })),
  );
  // The hubs and the home page are purged only after their entries are gone, so none is cached still listing the page.
  const purged = [...purgedFiles, ...(await deps.purger.purge(listUrls(deps, removed.hubs)))];
  return {
    status: 200,
    body: {
      page_id: gone.page_id,
      version: gone.version,
      status: "withdrawn",
      index_status: gone.index_status,
      purged,
    },
  };
}

/** A retry of a withdrawal that committed, now that a later version is live: purge again, answer as before. */
async function replayWithdraw(deps: Deps, done: OperationResult): Promise<OpResult> {
  const pageId = done.page_id as string;
  const version = await new Reader(deps.table()).getPageVersion(pageId, done.version);
  const purged = version ? await deps.purger.purge(affectedUrls(deps, version)) : [];
  return {
    status: 200,
    body: { page_id: pageId, version: done.version, status: "withdrawn", index_status: done.index_status, purged },
  };
}

export async function checkIndex(body: unknown): Promise<{ index_status: "index" | "noindex" }> {
  const req = check<{ index_status: "index" | "noindex" }>(schemas.validateIndexRequest, body);
  if (!req.ok) throw new ApiError("invalid_request", "the request body does not match indexRequest", req.issues);
  return req.value;
}

export async function prepareIndex(
  deps: Deps,
  pageId: string,
  req: { index_status: "index" | "noindex" },
): Promise<PreparedPageOp> {
  const page = await new Reader(deps.table()).getPage(pageId);
  if (!page) throw new ApiError("not_found", `page ${pageId} does not exist`);
  if (page.status === "withdrawn") throw new ApiError("page_withdrawn", `page ${pageId} is withdrawn`);
  return { page, indexStatus: req.index_status };
}

/**
 * Sets index_status on the live record (only while it is still the published version that was read, and only while
 * this request holds the page lock), its hub entries and its search row, then purges the page. Every step runs on
 * every attempt, so a retry finishes.
 */
export async function executeIndex(deps: Deps, p: PreparedPageOp, ctx: OpContext = unlocked(deps)): Promise<OpResult> {
  const table = deps.table();
  const { fence } = ctx;
  const status = p.indexStatus as "index" | "noindex";
  const page: PageRecord = { ...p.page, index_status: status };
  try {
    await fence.put(table, page as unknown as Doc, and(eq("version", page.version), eq("status", "published")));
  } catch (e) {
    if (e instanceof ConditionFailedError)
      throw new ApiError("version_conflict", "the page changed; read it again and retry");
    throw e;
  }
  await writeBatches(
    table,
    fence,
    page.hubs.map((h) => ({ put: hubEntry(page, h) as unknown as Doc })),
  );
  fence.check();
  await fence.bounded("search index status", deps.search.setIndexStatus(page, status));
  // The page's own URLs: its robots meta and sitemap entry follow index_status (ADR 0004).
  fence.check();
  const purged = await deps.purger.purge(pageFileUrls(deps.siteOrigin, page.path, ["index.html"]));
  return {
    status: 200,
    body: { page_id: page.page_id, version: page.version, status: page.status, index_status: status, purged },
  };
}
