// Everything the handlers touch, built once per request from the Worker's bindings. Tests build their own with fakes.
import { dynamoTableFromEnv } from "@news/worker-shared/dynamo/client";
import type { Table } from "@news/worker-shared/dynamo/table";
import { CloudflarePurger, type Purger } from "@news/worker-shared/purge";
import { D1SearchIndex, type SearchIndex } from "@news/worker-shared/search";
import { DEFAULT_SITE_ORIGIN, regionViews } from "@news/worker-shared/urls";
import { d1ReportStore, type ReportStore } from "./report-store";

export interface Env {
  PAGES?: R2Bucket;
  NEWS_DYNAMODB_TABLE?: string;
  NEWS_DYNAMODB_REGION?: string;
  NEWS_DYNAMODB_ENDPOINT?: string;
  NEWS_DYNAMODB_ACCESS_KEY_ID?: string;
  NEWS_DYNAMODB_SECRET_ACCESS_KEY?: string;
  /** The one write key (ADR 0006). A secret; never in wrangler.jsonc. */
  NEWS_WRITE_API_KEY?: string;
  CLOUDFLARE_ZONE_ID?: string;
  /** API token with only Zone > Cache Purge. A secret. */
  CLOUDFLARE_PURGE_TOKEN?: string;
  NEWS_SITE_ORIGIN?: string;
  NEWS_REGION_VIEWS?: string;
  /** Reader reports (D1, migrations/reports). Kept out of DynamoDB so reports cannot spend publishing capacity. */
  REPORTS?: D1Database;
  /** Reports accepted per UTC day across the whole site. Default 200. */
  NEWS_REPORT_DAILY_CAP?: string;
  AUTH_FAILURE_LIMITER?: RateLimit;
  REPORT_IP_LIMITER?: RateLimit;
  REPORT_GLOBAL_LIMITER?: RateLimit;
}

export interface Logger {
  warn(event: string, fields?: Record<string, unknown>): void;
  error(event: string, fields?: Record<string, unknown>): void;
}

/** Structured log lines. Callers pass IDs and codes only: never keys, bodies, e-mails or IP addresses. */
export const consoleLogger: Logger = {
  warn: (event, fields = {}) => console.warn(JSON.stringify({ level: "warn", event, ...fields })),
  error: (event, fields = {}) => console.error(JSON.stringify({ level: "error", event, ...fields })),
};

export interface Deps {
  /** Throws (as not_configured) when DynamoDB settings are missing. */
  table: () => Table;
  /** Throws (as not_configured) when the R2 binding is missing. */
  bucket: () => R2Bucket;
  search: SearchIndex;
  /** Throws (as not_configured) when the REPORTS binding is missing. */
  reports: () => ReportStore;
  reportDailyCap: number;
  purger: Purger;
  writeKey: string | undefined;
  siteOrigin: string;
  regions: string[];
  now: () => Date;
  limiters: { authFailure?: RateLimit; reportIp?: RateLimit; reportGlobal?: RateLimit };
  log: Logger;
}

export class NotConfiguredError extends Error {
  constructor(what: string) {
    super(`${what} is not configured`);
    this.name = "NotConfiguredError";
  }
}

/** Purges nothing and says so: local runs without a Cloudflare zone. Production sets the zone and token. */
const skippingPurger = (log: Logger): Purger => ({
  async purge(urls) {
    log.warn("purge_skipped", { reason: "CLOUDFLARE_ZONE_ID or CLOUDFLARE_PURGE_TOKEN not set", count: urls.length });
    return [...new Set(urls)];
  },
});

/** Loggers that already reported a bad purge setting: production has one logger, so that is once per isolate. */
const purgeConfigLogged = new WeakSet<Logger>();

/**
 * The Cloudflare purger, built on first use: a malformed zone ID fails only the requests that purge (as
 * not_configured, logged once per isolate), not every request including /health.
 */
function lazyPurger(zoneId: string, token: string, log: Logger): Purger {
  let purger: Purger | undefined;
  return {
    purge(urls) {
      if (!purger) {
        try {
          purger = new CloudflarePurger({ zoneId, token });
        } catch (e) {
          if (!purgeConfigLogged.has(log)) {
            purgeConfigLogged.add(log);
            log.error("purge_config", { message: e instanceof Error ? e.message : "invalid" });
          }
          return Promise.reject(new NotConfiguredError("cache purge"));
        }
      }
      return purger.purge(urls);
    },
  };
}

const ORIGIN = /^https:\/\/[a-z0-9.-]+$|^http:\/\/(?:localhost|127\.0\.0\.1)(?::[0-9]+)?$/;

export function depsFromEnv(env: Env, log: Logger = consoleLogger): Deps {
  let table: Table | undefined;
  const origin = env.NEWS_SITE_ORIGIN && ORIGIN.test(env.NEWS_SITE_ORIGIN) ? env.NEWS_SITE_ORIGIN : DEFAULT_SITE_ORIGIN;
  return {
    table: () => {
      if (!table) {
        try {
          table = dynamoTableFromEnv(env);
        } catch (e) {
          log.error("dynamodb_config", { message: e instanceof Error ? e.message : "invalid" });
          throw new NotConfiguredError("DynamoDB");
        }
      }
      return table;
    },
    bucket: () => {
      if (!env.PAGES) throw new NotConfiguredError("R2 bucket PAGES");
      return env.PAGES;
    },
    search: new D1SearchIndex(env),
    reports: () => {
      if (!env.REPORTS) throw new NotConfiguredError("D1 database REPORTS");
      return d1ReportStore(env.REPORTS);
    },
    reportDailyCap: /^[0-9]{1,6}$/.test(env.NEWS_REPORT_DAILY_CAP ?? "") ? Number(env.NEWS_REPORT_DAILY_CAP) : 200,
    purger:
      env.CLOUDFLARE_ZONE_ID && env.CLOUDFLARE_PURGE_TOKEN
        ? lazyPurger(env.CLOUDFLARE_ZONE_ID, env.CLOUDFLARE_PURGE_TOKEN, log)
        : skippingPurger(log),
    writeKey: env.NEWS_WRITE_API_KEY,
    siteOrigin: origin,
    regions: regionViews(env.NEWS_REGION_VIEWS),
    now: () => new Date(),
    limiters: {
      authFailure: env.AUTH_FAILURE_LIMITER,
      reportIp: env.REPORT_IP_LIMITER,
      reportGlobal: env.REPORT_GLOBAL_LIMITER,
    },
    log,
  };
}
