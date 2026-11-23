// The edge cache around a render (ADR 0010, section 3). A Worker's responses are not cached by the CDN on their own,
// so this looks a response up in the Workers Cache API under the route's canonical key, stores what it renders, serves
// the stored copy when the render or the data fails, and rate-limits the requests that miss the cache on the paths an
// attacker can multiply (404, cursor and filter). The Astro middleware calls it; nothing here knows about Astro.
import { cacheControl, type Match, POLICY, type Policy, type PolicyName, STALE_KEEP_SECONDS } from "./routes";

export type PageMatch = Extract<Match, { type: "page" }>;

/** The part of the Cache API this uses (`caches.default`). */
export interface EdgeCache {
  match(request: Request): Promise<Response | undefined>;
  put(request: Request, response: Response): Promise<void>;
}

export interface ServeDeps {
  cache?: EdgeCache;
  waitUntil(promise: Promise<unknown>): void;
  /** Epoch ms. */
  now(): number;
  /** The rate limiter: true when this client may spend another cache miss. Absent in local runs. */
  allow?(): Promise<boolean>;
  /** Renders the page from data (the next middleware step). */
  render(): Promise<Response>;
}

const FRESH_UNTIL = "x-news-fresh-until";
const STORED_POLICY = "x-news-policy";

const html503 =
  '<!doctype html><html lang="en-GB"><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><meta name="robots" content="noindex"><title>Try again shortly</title><p>This page could not be loaded just now. Please try again in a minute.</p></html>';

/** A response with its cache headers set from `policy` (a copy: stored and rendered responses are never mutated). */
function withPolicy(res: Response, policy: Policy | "no-store"): Response {
  const out = new Response(res.body, res);
  out.headers.delete(FRESH_UNTIL);
  out.headers.delete(STORED_POLICY);
  out.headers.set("cache-control", policy === "no-store" ? "no-store" : cacheControl(policy));
  return out;
}

function policyOf(m: PageMatch, status: number): PolicyName | undefined {
  if (status === 404) return "notFound";
  if (status === 301 || status === 308) return "redirect";
  if (status !== 200) return undefined;
  if (m.kind === "search") return "search";
  if (m.kind === "feed") return "feed";
  if (m.kind === "sitemap" || m.kind === "llms") return "daily";
  return m.purged ? (m.kind === "home" || m.kind === "region" ? "home" : "hub") : "hubPage";
}

export async function serve(request: Request, m: PageMatch, deps: ServeDeps): Promise<Response> {
  if (request.method !== "GET" && request.method !== "HEAD")
    return new Response("Method not allowed", {
      status: 405,
      headers: { allow: "GET, HEAD", "cache-control": "no-store" },
    });
  const keyRequest = new Request(m.key);
  let stale: Response | undefined;
  if (deps.cache) {
    const stored = await deps.cache.match(keyRequest);
    if (stored) {
      const name = stored.headers.get(STORED_POLICY) as PolicyName;
      if (deps.now() < Number(stored.headers.get(FRESH_UNTIL)))
        return withPolicy(stored, POLICY[name] ?? POLICY.notFound);
      stale = stored;
    }
  }
  if (m.limited && deps.allow && !(await deps.allow()))
    return new Response("Too many requests", {
      status: 429,
      headers: { "retry-after": "60", "cache-control": "no-store" },
    });
  let res: Response;
  try {
    res = await deps.render();
    // On search, a 5xx with Retry-After is an answer (the daily limit), not a failure to hide behind a stored copy.
    if (res.status >= 500 && !(m.kind === "search" && res.headers.has("retry-after"))) throw new Error(`render answered ${res.status}`);
  } catch (error) {
    console.error(
      JSON.stringify({
        event: "render_error",
        key: m.key,
        message: error instanceof Error ? error.message : String(error),
      }),
    );
    if (stale) return withPolicy(stale, POLICY.stale);
    return new Response(html503, {
      status: 503,
      headers: { "content-type": "text/html; charset=utf-8", "cache-control": "no-store" },
    });
  }
  const name = policyOf(m, res.status);
  if (!name) return withPolicy(res, "no-store");
  const policy = POLICY[name];
  if (deps.cache) {
    const kept = withPolicy(res.clone(), {
      edge: policy.edge + STALE_KEEP_SECONDS,
      browser: policy.edge + STALE_KEEP_SECONDS,
    });
    kept.headers.set(FRESH_UNTIL, String(deps.now() + policy.edge * 1000));
    kept.headers.set(STORED_POLICY, name);
    deps.waitUntil(deps.cache.put(keyRequest, kept));
  }
  return withPolicy(res, policy);
}
