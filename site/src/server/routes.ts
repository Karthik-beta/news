// The route table (ADR 0010, section 3): which URLs the site Worker renders from data, and each one's cache policy,
// in one place. `matchRoute` is pure (it reads no storage): it says what a URL is, rejects what cannot be a page, and
// gives the canonical cache key, so the key of `/?utm_source=x` is the key the api purges for `/`.
import { decodeCursor } from "@news/worker-shared/read";
import { parseSearchParams, type SearchParams, searchCacheKey } from "@news/worker-shared/search";
import { RESERVED_SLUGS } from "@news/worker-shared/urls";

/** Seconds. `edge` is `s-maxage` (the fresh time at the edge); `browser` is `max-age`. */
export interface Policy {
  edge: number;
  browser: number;
}

/**
 * Home and region views stay 5 minutes at the edge until #21 verifies on the real zone that a purge by URL evicts a
 * Cache API entry; then this one constant becomes 1 hour.
 */
export const HOME_EDGE_TTL = 300;

/** The cache policy of each kind of response. A test compares these with the table in ADR 0010. */
export const POLICY = {
  /** `/` and `/region/<code>`, first page. Purged by the api on a curation change. */
  home: { edge: HOME_EDGE_TTL, browser: 60 },
  /** `/latest`, a category hub, an entity hub: the first page, no query. Purged by the api. */
  hub: { edge: 86_400, browser: 60 },
  /** The same with `?after=` or `?region=`. Not purged. */
  hubPage: { edge: 3600, browser: 60 },
  /** A redirect from a `URL#` item. Purged when the redirect is written. */
  redirect: { edge: 86_400, browser: 86_400 },
  /** `/search`. Not purged. */
  search: { edge: 600, browser: 60 },
  /** RSS and Atom feeds. Not purged. */
  feed: { edge: 3600, browser: 900 },
  /** The sitemaps and `llms.txt`. Not purged. */
  daily: { edge: 86_400, browser: 3600 },
  notFound: { edge: 600, browser: 60 },
  /** A stored copy served because the render or the data failed. */
  stale: { edge: 30, browser: 30 },
} as const satisfies Record<string, Policy>;
export type PolicyName = keyof typeof POLICY;

/** How long a stored copy is kept past its fresh time, to serve when a render fails. */
export const STALE_KEEP_SECONDS = 7 * 86_400;

export const cacheControl = (p: Policy) => `public, max-age=${p.browser}, s-maxage=${p.edge}`;

const ENTITY_ID = /^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$/;
const SLUG_SEGMENT = /^[a-z0-9]+(?:-[a-z0-9]+)*$/;
/** A hub sort key: the first publication time, '#', the page ID. */
const CURSOR_KEY = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{1,3})?Z#[a-z0-9][a-z0-9-]*$/;

export type RouteKind = "home" | "region" | "latest" | "entity" | "category" | "search" | "feed" | "sitemap" | "llms";

export type Match =
  /** Not a route of this Worker (a static page, search or a feed from another part of the site). */
  | { type: "pass" }
  /** Answered without reading anything. */
  | { type: "reject"; status: 400 | 404; message: string }
  | { type: "redirect"; location: string }
  | {
      type: "page";
      kind: RouteKind;
      /** The path parts, decoded: ["economy", "inflation"], ["entity", "company:apple"], ["region", "india"]. */
      segments: string[];
      region?: string;
      /** A validated cursor (the base64url sort key). */
      after?: string;
      /** The canonical cache key: origin, canonical path, the route's known parameters in a fixed order. */
      key: string;
      /** A first page with no query: the url the api purges. */
      purged: boolean;
      /** Cache misses of this request count against the rate limiter (404, cursor and filter requests). */
      limited: boolean;
      /** `search`: the checked query and filters. */
      search?: SearchParams;
      /** `feed`: the format and the category slug (the site-wide feed has none). */
      feed?: { format: "rss" | "atom"; category?: string };
      /** `sitemap`: the month ("2025-01") of a child sitemap; none for the index. */
      month?: string;
    };

/** The canonical path: each segment encoded exactly as `hubPath()` writes it. */
export const canonicalPath = (segments: string[]) =>
  segments.length === 0 ? "/" : `/${segments.map(encodeURIComponent).join("/")}`;

/** The cache key of a URL on this site, or undefined when it is not one this Worker renders. */
export function cacheKeyOf(url: URL, origin: string, regions: readonly string[]): string | undefined {
  const m = matchRoute(url, origin, regions);
  return m.type === "page" ? m.key : undefined;
}

const notFound: Match = { type: "reject", status: 404, message: "no such page" };

export function matchRoute(url: URL, origin: string, regions: readonly string[]): Match {
  const path = url.pathname;
  // `//host` or `/\host` is a protocol-relative url, never a page: refuse it before any redirect can point off-site.
  if (path.startsWith("//") || path.startsWith("/\\")) return notFound;
  // A hub has one URL: a trailing slash answers 301 to the path without it.
  if (path.length > 1 && path.endsWith("/"))
    return { type: "redirect", location: `/${path.replace(/^\/+|\/+$/g, "")}${url.search}` };
  let segments: string[];
  try {
    segments = path === "/" ? [] : path.slice(1).split("/").map(decodeURIComponent);
  } catch {
    return notFound;
  }
  if (segments.length > 4 + 1 || segments.some((s) => s === "")) return notFound;

  /** Validates the parameters a route knows; the rest (utm_*) are ignored and left out of the key. */
  const page = (kind: RouteKind, own: { region?: boolean; after?: boolean }): Match => {
    const q = new URLSearchParams();
    let region: string | undefined;
    let after: string | undefined;
    if (own.region && url.searchParams.has("region")) {
      region = url.searchParams.get("region") ?? "";
      if (!regions.includes(region)) return notFound;
      q.set("region", region);
    }
    if (own.after && url.searchParams.has("after")) {
      after = url.searchParams.get("after") ?? "";
      try {
        if (!CURSOR_KEY.test(decodeCursor("", after)?.SK ?? "")) throw new RangeError("not a hub sort key");
      } catch {
        return { type: "reject", status: 400, message: "the cursor is not valid" };
      }
      q.set("after", after);
    }
    return {
      type: "page",
      kind,
      segments,
      ...(region ? { region } : {}),
      ...(after ? { after } : {}),
      key: `${origin}${canonicalPath(segments)}${q.size > 0 ? `?${q}` : ""}`,
      purged: q.size === 0,
      limited: kind === "category" || kind === "entity" || q.size > 0,
    };
  };

  /** A page the Worker renders that is never purged: it has its own key and no cursor or region view. */
  const other = (kind: RouteKind, key: string, extra: Partial<Extract<Match, { type: "page" }>> = {}): Match => ({
    type: "page",
    kind,
    segments,
    key: `${origin}${key}`,
    purged: false,
    limited: false,
    ...extra,
  });

  const [first, second] = segments;
  const last = segments[segments.length - 1];
  if (first === undefined) return page("home", {});
  if (first === "search") {
    if (segments.length > 1) return notFound;
    const parsed = parseSearchParams(url.searchParams, { optionalQuery: true });
    if (!parsed.ok) return { type: "reject", status: 400, message: parsed.message };
    const { limit: _limit, ...params } = parsed.params;
    return other("search", params.query === "" ? "/search" : `/search?${searchCacheKey(params)}`, {
      search: params,
      limited: true,
    });
  }
  if (segments.length === 1 && first === "sitemap.xml") return other("sitemap", "/sitemap.xml");
  if (segments.length === 1 && first === "llms.txt") return other("llms", "/llms.txt");
  if (first === "sitemaps") {
    const month = /^(\d{4}-(?:0[1-9]|1[0-2]))\.xml$/.exec(second ?? "")?.[1];
    return segments.length === 2 && month
      ? other("sitemap", `/sitemaps/${month}.xml`, { month, limited: true })
      : notFound;
  }
  if (last === "rss.xml" || last === "atom.xml") {
    const category = segments.slice(0, -1);
    if (category.length > 4 || !category.every((s) => SLUG_SEGMENT.test(s))) return notFound;
    const slug = category.join("/");
    return other("feed", `/${slug === "" ? "" : `${slug}/`}${last}`, {
      feed: { format: last === "rss.xml" ? "rss" : "atom", ...(slug ? { category: slug } : {}) },
      limited: slug !== "",
    });
  }
  if (first === "latest") return segments.length === 1 ? page("latest", { region: true, after: true }) : notFound;
  if (first === "region")
    return segments.length === 2 && regions.includes(second as string) ? page("region", { after: true }) : notFound;
  if (first === "entity")
    return segments.length === 2 && ENTITY_ID.test(second as string) ? page("entity", { after: true }) : notFound;
  if (RESERVED_SLUGS.includes(first)) return { type: "pass" };
  if (segments.length > 4 || !segments.every((s) => SLUG_SEGMENT.test(s))) return notFound;
  return page("category", { region: true, after: true });
}
