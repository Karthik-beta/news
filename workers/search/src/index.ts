// news-search: the public search API over D1 (contracts/cloud/search-api.openapi.json). Read-only: GET /v1/search.
// One implementation with the site's /search page (workers/shared/src/search.ts); this Worker is the thin JSON caller.
// Popular queries are cached at the edge under a normalised key (ADR 0010, section 7).
import { clientIp, json } from "@news/worker-shared/http";
import {
  listShards,
  parseSearchParams,
  QueryTooLongError,
  SearchUnavailableError,
  searchCacheKey,
  searchPages,
} from "@news/worker-shared/search";
import { absolute, DEFAULT_SITE_ORIGIN } from "@news/worker-shared/urls";

export interface Env {
  NEWS_SITE_ORIGIN?: string;
  SEARCH_RATE_LIMITER?: { limit(options: { key: string }): Promise<{ success: boolean }> };
  // The SEARCH_<YYYY> D1 shards are found by name (listShards).
}

/** Fresh time of a cached answer, seconds. */
const CACHE_SECONDS = 600;
const error = (status: number, code: string, message: string, headers: Record<string, string> = {}) =>
  json({ error: { code, message } }, status, headers);

/** Seconds until the next UTC midnight, when the free database's daily limit resets. */
const untilTomorrow = (now: Date) =>
  String(Math.ceil((Date.UTC(now.getUTCFullYear(), now.getUTCMonth(), now.getUTCDate() + 1) - now.getTime()) / 1000));

export default {
  async fetch(request: Request, bindings: unknown, ctx: ExecutionContext): Promise<Response> {
    const env = bindings as Env;
    const url = new URL(request.url);
    if (url.pathname === "/health") return Response.json({ worker: "news-search", status: "ok" });
    if (url.pathname !== "/v1/search")
      return Response.json({ error: "Not implemented yet (Phase 6)" }, { status: 501 });
    if (request.method !== "GET" && request.method !== "HEAD")
      return error(405, "method_not_allowed", "use GET", { allow: "GET, HEAD" });

    const parsed = parseSearchParams(url.searchParams);
    if (!parsed.ok) return error(400, "invalid_request", parsed.message);
    const params = { limit: 10, ...parsed.params };

    // The edge cache first: a popular query costs no D1 read and no rate-limit token.
    const origin = (env.NEWS_SITE_ORIGIN ?? DEFAULT_SITE_ORIGIN).replace(/\/+$/, "");
    const key = new Request(`${url.origin}/v1/search?${searchCacheKey(params)}`);
    const cache = (caches as unknown as { default: Cache }).default;
    const hit = await cache.match(key);
    if (hit) return hit;

    if (env.SEARCH_RATE_LIMITER && !(await env.SEARCH_RATE_LIMITER.limit({ key: clientIp(request) })).success)
      return error(429, "rate_limited", "too many searches; try again in a minute", { "retry-after": "60" });

    try {
      const hits = await searchPages(listShards(env), params);
      const res = json({ results: hits.map(({ path, ...h }) => ({ ...h, url: absolute(origin, path) })) }, 200, {
        "cache-control": `public, max-age=60, s-maxage=${CACHE_SECONDS}`,
      });
      ctx.waitUntil(cache.put(key, res.clone()));
      return res;
    } catch (e) {
      if (e instanceof QueryTooLongError) return error(400, "invalid_request", e.message);
      if (e instanceof SearchUnavailableError)
        return error(503, "search_unavailable", "search has a daily limit and is unavailable until 00:00 UTC", {
          "retry-after": untilTomorrow(new Date()),
        });
      throw e;
    }
  },
} satisfies ExportedHandler;
