// D1 full-text search (SQLite FTS5): the shard router, index writes and the query module.
// One D1 database per publication year, bound as SEARCH_<YYYY> (ADR 0004): each free database holds 500 MB.
import { categoryLineage } from "./keys";
import type { PageRecord } from "./records";

export const SHARD_BINDING = /^SEARCH_([0-9]{4})$/;

export interface SearchShard {
  year: number;
  db: D1Database;
}

/** Every SEARCH_<YYYY> binding in `env`, newest year first. */
export function listShards(env: object): SearchShard[] {
  const shards: SearchShard[] = [];
  for (const [name, value] of Object.entries(env)) {
    const m = SHARD_BINDING.exec(name);
    if (m?.[1] && value && typeof (value as D1Database).prepare === "function")
      shards.push({ year: Number(m[1]), db: value as D1Database });
  }
  return shards.sort((a, b) => b.year - a.year);
}

export class MissingShardError extends Error {
  constructor(readonly year: number) {
    super(`no search shard is bound for ${year} (binding SEARCH_${year})`);
    this.name = "MissingShardError";
  }
}

/** The shard a page belongs to: the year of its first publication. */
export function shardFor(env: object, publishedAt: string): D1Database {
  const year = Number(publishedAt.slice(0, 4));
  const shard = listShards(env).find((s) => s.year === year);
  if (!shard) throw new MissingShardError(year);
  return shard.db;
}

const pad = (tokens: string[]) => (tokens.length > 0 ? ` ${tokens.join(" ")} ` : " ");

/** Inserts or replaces the page's search row (only published pages are indexed). */
export async function indexPage(db: D1Database, page: PageRecord): Promise<void> {
  const categories = [...new Set(page.categories.flatMap(categoryLineage))];
  await db
    .prepare(
      `INSERT INTO pages (page_id, path, lang, title, summary, entities, tags, categories, region, page_type, tier,
         published_at, updated_at, index_status)
       VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
       ON CONFLICT (page_id) DO UPDATE SET path = excluded.path, lang = excluded.lang, title = excluded.title,
         summary = excluded.summary, entities = excluded.entities, tags = excluded.tags,
         categories = excluded.categories, region = excluded.region, page_type = excluded.page_type,
         tier = excluded.tier, published_at = excluded.published_at, updated_at = excluded.updated_at,
         index_status = excluded.index_status`,
    )
    .bind(
      page.page_id,
      page.path,
      page.lang,
      page.title,
      page.summary,
      page.entities.map((e) => e.name).join(" | "),
      page.tags.join(" | "),
      pad(categories),
      page.region,
      page.page_type,
      page.tier,
      page.published_at,
      page.updated_at,
      page.index_status,
    )
    .run();
}

export async function removePage(db: D1Database, pageId: string): Promise<void> {
  await db.prepare("DELETE FROM pages WHERE page_id = ?1").bind(pageId).run();
}

export async function setIndexStatus(db: D1Database, pageId: string, status: "index" | "noindex"): Promise<void> {
  await db.prepare("UPDATE pages SET index_status = ?2 WHERE page_id = ?1").bind(pageId, status).run();
}

export interface SearchParams {
  query: string;
  category?: string;
  region?: string;
  page_type?: string;
  /** YYYY-MM-DD, inclusive. */
  from?: string;
  /** YYYY-MM-DD, inclusive. */
  to?: string;
  limit?: number;
}

export interface SearchHit {
  page_id: string;
  path: string;
  lang: string;
  title: string;
  summary: string;
  categories: string[];
  region: string;
  page_type: string;
  published_at: string;
}

export const MAX_TERMS = 8;
/** The shortest last word searched as a prefix: "e*" would walk a large part of the index for one query. */
const MIN_PREFIX = 3;

/** More words than a query may have. Refused rather than cut, because every word must match. */
export class QueryTooLongError extends RangeError {
  constructor() {
    super(`a search takes at most ${MAX_TERMS} words`);
    this.name = "QueryTooLongError";
  }
}

/**
 * Turns free text into a safe FTS5 query: words only (letters and digits), each quoted so no FTS5 operator or
 * column filter can be injected, all required, the last one as a prefix when it has at least 3 characters.
 * Returns null when nothing searchable is left; throws QueryTooLongError above MAX_TERMS words.
 */
export function toFtsQuery(text: string): string | null {
  // No normalisation here: indexed text is stored as written, so the query is matched as written too (FTS5's
  // unicode61 tokenizer folds case and diacritics on both sides). Combining marks stay inside their word, as unicode61
  // keeps them ("naïve", Devanagari vowel signs); a mark with no letter before it is dropped.
  const words = text.match(/[\p{L}\p{N}][\p{L}\p{N}\p{M}]*/gu) ?? [];
  if (words.length > MAX_TERMS) throw new QueryTooLongError();
  if (words.length === 0) return null;
  const last = words.length - 1;
  return words.map((w, i) => `"${w}"${i === last && [...w].length >= MIN_PREFIX ? "*" : ""}`).join(" ");
}

const dayEnd = (d: string) => `${d}T23:59:59.999Z`;

/** Searches the newest shards first and stops once `limit` hits are found. */
export async function searchPages(shards: SearchShard[], params: SearchParams): Promise<SearchHit[]> {
  const match = toFtsQuery(params.query);
  if (!match) return [];
  const limit = Math.min(Math.max(params.limit ?? 10, 1), 50);
  const where = ["pages_fts MATCH ?1"];
  const binds: unknown[] = [match];
  const add = (sql: string, value: unknown) => {
    binds.push(value);
    where.push(sql.replace("?", `?${binds.length}`));
  };
  if (params.category) add("instr(p.categories, ' ' || ? || ' ') > 0", params.category);
  if (params.region) add("p.region = ?", params.region);
  if (params.page_type) add("p.page_type = ?", params.page_type);
  if (params.from) add("p.published_at >= ?", params.from);
  if (params.to) add("p.published_at <= ?", dayEnd(params.to));
  const fromYear = params.from ? Number(params.from.slice(0, 4)) : Number.NEGATIVE_INFINITY;
  const toYear = params.to ? Number(params.to.slice(0, 4)) : Number.POSITIVE_INFINITY;

  const hits: SearchHit[] = [];
  for (const shard of shards) {
    if (shard.year < fromYear || shard.year > toYear) continue;
    const { results } = await shard.db
      .prepare(
        `SELECT p.page_id, p.path, p.lang, p.title, p.summary, p.categories, p.region, p.page_type, p.published_at
         FROM pages_fts JOIN pages p ON p.rowid = pages_fts.rowid
         WHERE ${where.join(" AND ")}
         ORDER BY bm25(pages_fts, 10.0, 4.0, 3.0, 2.0, 1.0), p.published_at DESC
         LIMIT ${limit - hits.length}`,
      )
      .bind(...binds)
      .all<Omit<SearchHit, "categories"> & { categories: string }>();
    for (const row of results) hits.push({ ...row, categories: row.categories.trim().split(" ").filter(Boolean) });
    if (hits.length >= limit) break;
  }
  return hits;
}

/** What the write API needs from the search index. */
export interface SearchIndex {
  /** Throws MissingShardError when no shard is bound for the page's year (checked before anything is written). */
  assertShard(publishedAt: string): void;
  index(page: PageRecord): Promise<void>;
  remove(page: Pick<PageRecord, "page_id" | "published_at">): Promise<void>;
  setIndexStatus(page: Pick<PageRecord, "page_id" | "published_at">, status: "index" | "noindex"): Promise<void>;
  /** Path and language of a published page (its search row exists exactly while it is published), or undefined. */
  findPage(pageId: string): Promise<{ path: string; lang: string } | undefined>;
}

/** The D1 shards bound to the Worker (SEARCH_<YYYY>). */
export class D1SearchIndex implements SearchIndex {
  constructor(readonly env: object) {}
  assertShard(publishedAt: string): void {
    shardFor(this.env, publishedAt);
  }
  index(page: PageRecord): Promise<void> {
    return indexPage(shardFor(this.env, page.published_at), page);
  }
  remove(page: Pick<PageRecord, "page_id" | "published_at">): Promise<void> {
    return removePage(shardFor(this.env, page.published_at), page.page_id);
  }
  setIndexStatus(page: Pick<PageRecord, "page_id" | "published_at">, status: "index" | "noindex"): Promise<void> {
    return setIndexStatus(shardFor(this.env, page.published_at), page.page_id, status);
  }
  async findPage(pageId: string): Promise<{ path: string; lang: string } | undefined> {
    for (const shard of listShards(this.env)) {
      const row = await shard.db
        .prepare("SELECT path, lang FROM pages WHERE page_id = ?1")
        .bind(pageId)
        .first<{ path: string; lang: string }>();
      if (row) return { path: row.path, lang: row.lang };
    }
    return undefined;
  }
}
