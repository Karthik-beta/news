// The only module of the site that touches storage (ADR 0010, section 2). It reuses #6's code (the read layer, the
// DynamoDB client, R2 range reads, the search shards) and adds no second read layer.
//   NEWS_DATA=dynamodb (production)  DynamoDB with strongly consistent reads: the api purges right after its writes, and
//                                    an eventually consistent read in that window would cache the old page for its TTL.
//   NEWS_DATA=local (dev, tests)     a fixture set in this isolate's memory (./local); refused in production.
import { dynamoTableFromEnv } from "@news/worker-shared/dynamo/client";
import { pageObjectKey, readCapped } from "@news/worker-shared/r2";
import { Reader } from "@news/worker-shared/read";
import type { Curation } from "@news/worker-shared/records";
import { listShards, type SearchShard } from "@news/worker-shared/search";

/** Cloudflare's R2 bucket binding, as the shared R2 helpers take it (the global type is not loaded here). */
type R2Bucket = Parameters<typeof readCapped>[0];

/** The Worker's bindings and settings (site/wrangler.jsonc). Search shards are `SEARCH_<YYYY>` D1 bindings. */
export interface SiteEnv {
  NEWS_ENV?: string;
  NEWS_DATA?: string;
  /** Local mode only: the fixture set in site/fixtures/data (default `normal`). */
  NEWS_FIXTURE_SET?: string;
  NEWS_SITE_ORIGIN?: string;
  NEWS_REGION_VIEWS?: string;
  NEWS_DYNAMODB_TABLE?: string;
  NEWS_DYNAMODB_REGION?: string;
  NEWS_DYNAMODB_ENDPOINT?: string;
  NEWS_DYNAMODB_ACCESS_KEY_ID?: string;
  NEWS_DYNAMODB_SECRET_ACCESS_KEY?: string;
  PAGES?: R2Bucket;
  SEARCH_RATE_LIMITER?: { limit(options: { key: string }): Promise<{ success: boolean }> };
}

export interface SiteData {
  /** The shared read layer. Every read is strongly consistent. */
  reader: Reader;
  /** The live curation (`HOME#CURATION` LATEST), or undefined before the first one. */
  curation(): Promise<Curation | undefined>;
  /** `SEARCH_<YYYY>` bindings, newest first. */
  shards: SearchShard[];
  /** A page's file from R2 (get only), parsed. Undefined when it is missing, larger than `maxBytes` or not JSON. */
  pageJson(path: string, file: string, maxBytes: number): Promise<unknown | undefined>;
  /** The same for a text file (`page.oui`). */
  pageText(path: string, file: string, maxBytes: number): Promise<string | undefined>;
}

/** Environments in which `NEWS_DATA=local` is allowed. Anything else, including an unset NEWS_ENV, is production. */
const LOCAL_ENVS = new Set(["local", "test"]);

let local: Promise<SiteData> | undefined;
let production: { key: string; data: SiteData } | undefined;

/** The data adapter for this environment. Throws (at the first request) when the settings are not allowed. */
export async function siteData(env: SiteEnv): Promise<SiteData> {
  const mode = env.NEWS_DATA ?? "dynamodb";
  if (mode === "local") {
    if (!LOCAL_ENVS.has(env.NEWS_ENV ?? "")) throw new Error("NEWS_DATA=local is refused outside NEWS_ENV=local");
    // Loaded on demand so the production path never evaluates the fixture seed.
    local ??= import("./local").then((m) => m.localSiteData(env));
    return local;
  }
  if (mode !== "dynamodb") throw new Error(`NEWS_DATA must be dynamodb or local, not ${JSON.stringify(mode)}`);
  const key = `${env.NEWS_DYNAMODB_TABLE}/${env.NEWS_DYNAMODB_REGION}/${env.NEWS_DYNAMODB_ENDPOINT ?? ""}`;
  if (production?.key !== key) production = { key, data: dynamoSiteData(env) };
  return production.data;
}

function dynamoSiteData(env: SiteEnv): SiteData {
  const reader = new Reader(dynamoTableFromEnv(env, { consistentRead: true }));
  const bucket = env.PAGES;
  const read = async (path: string, file: string, maxBytes: number) => {
    if (!bucket) throw new Error("the PAGES R2 binding is not set");
    const got = await readCapped(bucket, pageObjectKey(path, file), maxBytes);
    return got && !got.truncated ? got.text : undefined;
  };
  return {
    reader,
    curation: () => reader.getLiveCuration(),
    shards: listShards(env),
    pageText: read,
    async pageJson(path, file, maxBytes) {
      const text = await read(path, file, maxBytes);
      if (text === undefined) return undefined;
      try {
        return JSON.parse(text);
      } catch {
        return undefined;
      }
    },
  };
}
