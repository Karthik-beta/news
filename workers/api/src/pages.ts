// Page operations: publish (PUT /v1/pages/{id}), withdraw, and index promotion.
import { sha256Hex } from "@news/worker-shared/crypto";
import { eq, le, notExists, or } from "@news/worker-shared/dynamo/conditions";
import { ConditionFailedError, type Doc, itemSize } from "@news/worker-shared/dynamo/table";
import {
  entityKey,
  hubPk,
  hubSk,
  hubsForPage,
  pageLatestKey,
  pageVersionKey,
  pathClaimKey,
} from "@news/worker-shared/keys";
import { deletePageFiles, putPageFile } from "@news/worker-shared/r2";
import { Reader } from "@news/worker-shared/read";
import type { EntityRecord, HubEntry, Manifest, PageMeta, PageRecord, PathClaim } from "@news/worker-shared/records";
import { MissingShardError } from "@news/worker-shared/search";
import { absolute, HOME_PATH, hubPath, pageFileUrls } from "@news/worker-shared/urls";
import { check, type Issue, schemas } from "@news/worker-shared/validate";
import type { Deps } from "./deps";
import { ApiError } from "./errors";

export const REQUIRED_FILES = ["index.html", "index.md", "data.json", "sources.json", "meta.json", "page.oui"];
/** Page record items stay far below DynamoDB's 400 KB: metadata only, files in R2 (ADR 0004). */
const MAX_PAGE_ITEM_BYTES = 64 * 1024;
const CONCURRENCY = 6;

export interface OpResult {
  status: number;
  body: unknown;
}

/** Runs `fn` over `items` with at most `n` in flight (keeps subrequests and DynamoDB bursts small). */
async function eachLimited<T>(items: T[], n: number, fn: (item: T) => Promise<unknown>): Promise<void> {
  let next = 0;
  const worker = async () => {
    while (next < items.length) await fn(items[next++] as T);
  };
  await Promise.all(Array.from({ length: Math.min(n, items.length) }, worker));
}

// --- publish ------------------------------------------------------------------------------------------------------

export interface PreparedPage {
  meta: PageMeta;
  manifest: Manifest;
  files: Record<string, string>;
  previous: PageRecord | undefined;
}

const enc = new TextEncoder();

/** Validates the whole bundle (no writes): schema, hashes, meta, and that it may replace the live version. */
export async function preparePut(deps: Deps, pageId: string, body: unknown): Promise<PreparedPage> {
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
  const meta = m.value;
  if (meta.id !== pageId) bad("meta.id", "meta.id differs from the URL", "/id");
  if (meta.version !== manifest.version) bad("meta.version", "meta.version differs from manifest.version", "/version");
  if (meta.updated_at < meta.published_at) bad("meta.updated_at", "updated_at is before published_at", "/updated_at");
  for (const name of Object.keys(files)) {
    const chart = /^charts\/(.+)\.csv$/.exec(name)?.[1];
    if (chart && !meta.chart_ids.includes(chart))
      bad("meta.chart_ids", `charts/${chart}.csv is not in chart_ids`, "/chart_ids");
  }
  if (issues.length > 0) throw new ApiError("invalid_meta", "meta.json does not fit the bundle", issues);

  try {
    deps.search.assertShard(meta.published_at);
  } catch (e) {
    if (e instanceof MissingShardError) {
      deps.log.error("search_shard_missing", { year: e.year });
      throw new ApiError("not_configured", `no search shard is bound for ${e.year}`);
    }
    throw e;
  }

  const previous = await new Reader(deps.table()).getPage(pageId);
  if (previous) assertCanReplace(previous, meta, manifest);
  return { meta, manifest, files, previous };
}

function assertCanReplace(previous: PageRecord, meta: PageMeta, manifest: Manifest): void {
  if (previous.path !== meta.path)
    throw new ApiError(
      "path_conflict",
      `page ${meta.id} is published at ${previous.path}; a page never changes its URL`,
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
): PageRecord {
  const { schema: _schema, id, ...fields } = meta;
  // A page keeps its first publication time, so its hub entries keep their place and SK.
  const publishedAt = previous?.published_at ?? meta.published_at;
  return {
    ...pageLatestKey(id),
    type: "page",
    ...fields,
    page_id: id,
    published_at: publishedAt,
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

/** URLs to purge after a page changes: its files, every hub it is or was in, and the home page. */
function affectedUrls(deps: Deps, page: PageRecord, previous: PageRecord | undefined): string[] {
  const hubs = new Set([...page.hubs, ...(previous?.hubs ?? [])]);
  const files = new Set([...page.files, ...(previous?.files ?? [])]);
  return [
    ...pageFileUrls(deps.siteOrigin, page.path, [...files]),
    ...[...hubs].map((h) => absolute(deps.siteOrigin, hubPath(h))),
    absolute(deps.siteOrigin, HOME_PATH),
  ];
}

export async function executePut(deps: Deps, p: PreparedPage): Promise<OpResult> {
  const table = deps.table();
  const bucket = deps.bucket();
  const now = deps.now();
  const page = pageRecord(p.meta, p.manifest, p.previous, now);
  if (itemSize(page as unknown as Doc) > MAX_PAGE_ITEM_BYTES)
    throw new ApiError("invalid_meta", "page metadata is too large for one record (64 KB)");

  // 1. Claim the path for this page, so no other page can publish at the same URL (or R2 prefix).
  const claimItem: PathClaim = {
    ...pathClaimKey(page.path),
    SK: "PAGE",
    type: "path_claim",
    lang: page.lang,
    page_id: page.page_id,
    path: page.path,
    created_at: now.toISOString(),
  };
  try {
    await table.put(claimItem as never, or(notExists(), eq("page_id", page.page_id)));
  } catch (e) {
    if (e instanceof ConditionFailedError) throw new ApiError("path_conflict", `${page.path} belongs to another page`);
    throw e;
  }

  // 2. The files, then stale chart files of the previous version.
  await eachLimited(Object.entries(p.files), CONCURRENCY, ([name, text]) => putPageFile(bucket, page.path, name, text));
  const stale = (p.previous?.files ?? []).filter((f) => !page.files.includes(f));
  if (stale.length > 0) await deletePageFiles(bucket, page.path, stale);

  // 3. The immutable version record; an identical retry passes, different content under the same version does not.
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

  // 4. Hubs and entities. Entries of hubs the page left are removed.
  const entries = page.hubs.map((h) => hubEntry(page, h));
  const keep = new Set(entries.map((e) => `${e.PK}\u0000${e.SK}`));
  const removed = (p.previous?.hubs ?? [])
    .map((h) => ({ PK: hubPk(h), SK: hubSk(p.previous?.published_at ?? page.published_at, page.page_id) }))
    .filter((k) => !keep.has(`${k.PK}\u0000${k.SK}`));
  await eachLimited(entries, CONCURRENCY, (e) => table.put(e as never));
  await eachLimited(removed, CONCURRENCY, (k) => table.delete(k));
  const entities: EntityRecord[] = p.meta.entities.map((e) => ({
    ...entityKey(e.id),
    SK: "META",
    type: "entity",
    lang: page.lang,
    id: e.id,
    name: e.name,
    kind: e.kind,
    updated_at: now.toISOString(),
  }));
  await eachLimited(entities, CONCURRENCY, (e) => table.put(e as never));

  // 5. Search row, then LATEST last: moving LATEST is the commit point readers see.
  await deps.search.index(page);
  try {
    await table.put(page as never, or(notExists(), le("version", page.version)));
  } catch (e) {
    if (e instanceof ConditionFailedError)
      throw new ApiError("version_conflict", "a newer version was published while this one was being written");
    throw e;
  }

  const purged = await deps.purger.purge(affectedUrls(deps, page, p.previous));
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
}

export async function prepareWithdraw(deps: Deps, pageId: string, body: unknown): Promise<PreparedPageOp> {
  const req = check<{ reason: string }>(schemas.validateWithdrawRequest, body);
  if (!req.ok) throw new ApiError("invalid_request", "the request body does not match withdrawRequest", req.issues);
  const page = await new Reader(deps.table()).getPage(pageId);
  if (!page) throw new ApiError("not_found", `page ${pageId} does not exist`);
  return { page, reason: req.value.reason };
}

export async function executeWithdraw(deps: Deps, p: PreparedPageOp): Promise<OpResult> {
  const table = deps.table();
  const now = deps.now();
  const page = p.page;
  const body = (status: PageRecord["status"], purged: string[]) => ({
    page_id: page.page_id,
    version: page.version,
    status,
    index_status: page.index_status,
    purged,
  });
  // Already withdrawn: nothing to change.
  if (page.status === "withdrawn") return { status: 200, body: body("withdrawn", []) };

  const withdrawn: PageRecord = {
    ...page,
    status: "withdrawn",
    withdrawn_at: now.toISOString(),
    withdraw_reason: p.reason,
  };
  try {
    await table.put(withdrawn as never, eq("version", page.version));
  } catch (e) {
    if (e instanceof ConditionFailedError)
      throw new ApiError("version_conflict", "the page changed; read it again and retry");
    throw e;
  }
  await eachLimited(page.hubs, CONCURRENCY, (h) =>
    table.delete({ PK: hubPk(h), SK: hubSk(page.published_at, page.page_id) }),
  );
  await deps.search.remove(page);
  // The public files go: a withdrawn page must stop being served. Versions stay in DynamoDB and the bundle on the laptop.
  await deletePageFiles(deps.bucket(), page.path, page.files);
  const purged = await deps.purger.purge(affectedUrls(deps, page, undefined));
  return { status: 200, body: body("withdrawn", purged) };
}

export async function prepareIndex(deps: Deps, pageId: string, body: unknown): Promise<PreparedPageOp> {
  const req = check<{ index_status: "index" | "noindex" }>(schemas.validateIndexRequest, body);
  if (!req.ok) throw new ApiError("invalid_request", "the request body does not match indexRequest", req.issues);
  const page = await new Reader(deps.table()).getPage(pageId);
  if (!page) throw new ApiError("not_found", `page ${pageId} does not exist`);
  if (page.status === "withdrawn") throw new ApiError("page_withdrawn", `page ${pageId} is withdrawn`);
  return { page, indexStatus: req.value.index_status };
}

export async function executeIndex(deps: Deps, p: PreparedPageOp): Promise<OpResult> {
  const table = deps.table();
  const status = p.indexStatus as "index" | "noindex";
  const page: PageRecord = { ...p.page, index_status: status };
  const result = (purged: string[]) => ({
    page_id: page.page_id,
    version: page.version,
    status: page.status,
    index_status: status,
    purged,
  });
  if (p.page.index_status === status) return { status: 200, body: result([]) };
  try {
    await table.put(page as never, eq("version", page.version));
  } catch (e) {
    if (e instanceof ConditionFailedError)
      throw new ApiError("version_conflict", "the page changed; read it again and retry");
    throw e;
  }
  await eachLimited(page.hubs, CONCURRENCY, (h) => table.put(hubEntry(page, h) as never));
  await deps.search.setIndexStatus(page, status);
  // The page's own URLs: its robots meta and sitemap entry follow index_status (ADR 0004).
  const purged = await deps.purger.purge(pageFileUrls(deps.siteOrigin, page.path, ["index.html"]));
  return { status: 200, body: result(purged) };
}
