// What the MCP server reads, built from the Worker's bindings. Every binding here is read-only in use: the DynamoDB
// key is meant to be a read-only IAM key (ADR 0006), and the code below only ever reads R2 and SELECTs from D1.
import { dynamoTableFromEnv } from "@news/worker-shared/dynamo/client";
import { Reader } from "@news/worker-shared/read";
import { listShards, type SearchShard } from "@news/worker-shared/search";
import { DEFAULT_SITE_ORIGIN } from "@news/worker-shared/urls";

export interface Env {
  PAGES?: R2Bucket;
  NEWS_DYNAMODB_TABLE?: string;
  NEWS_DYNAMODB_REGION?: string;
  NEWS_DYNAMODB_ENDPOINT?: string;
  NEWS_DYNAMODB_ACCESS_KEY_ID?: string;
  NEWS_DYNAMODB_SECRET_ACCESS_KEY?: string;
  NEWS_SITE_ORIGIN?: string;
  /** Comma-separated origins allowed to call from a browser. Requests without an Origin header are always allowed. */
  NEWS_MCP_ALLOWED_ORIGINS?: string;
  MCP_RATE_LIMITER?: RateLimit;
}

export class UnavailableError extends Error {
  constructor(what: string) {
    super(`${what} is not available`);
    this.name = "UnavailableError";
  }
}

export interface Logger {
  error(event: string, fields?: Record<string, unknown>): void;
}

export interface Deps {
  reader: () => Reader;
  bucket: () => R2Bucket;
  shards: () => SearchShard[];
  limiter?: RateLimit;
  siteOrigin: string;
  allowedOrigins: Set<string>;
  log: Logger;
}

const ORIGIN = /^https:\/\/[a-z0-9.-]+$|^http:\/\/(?:localhost|127\.0\.0\.1)(?::[0-9]+)?$/;

export function depsFromEnv(env: Env): Deps {
  const log: Logger = {
    error: (event, fields = {}) => console.error(JSON.stringify({ level: "error", event, ...fields })),
  };
  const siteOrigin =
    env.NEWS_SITE_ORIGIN && ORIGIN.test(env.NEWS_SITE_ORIGIN) ? env.NEWS_SITE_ORIGIN : DEFAULT_SITE_ORIGIN;
  const allowed = (env.NEWS_MCP_ALLOWED_ORIGINS ?? "")
    .split(",")
    .map((s) => s.trim())
    .filter((s) => ORIGIN.test(s));
  let reader: Reader | undefined;
  return {
    reader: () => {
      if (!reader) {
        try {
          // Eventually consistent: half the read units of the shared table, and a public reader can lag a second.
          reader = new Reader(dynamoTableFromEnv(env, { consistentRead: false }));
        } catch (e) {
          log.error("dynamodb_config", { message: e instanceof Error ? e.message : "invalid" });
          throw new UnavailableError("the page database");
        }
      }
      return reader;
    },
    bucket: () => {
      if (!env.PAGES) throw new UnavailableError("page storage");
      return env.PAGES;
    },
    shards: () => listShards(env),
    limiter: env.MCP_RATE_LIMITER,
    siteOrigin,
    allowedOrigins: new Set([siteOrigin, ...allowed]),
    log,
  };
}
