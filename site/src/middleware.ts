// Applies the route table to every request the Worker renders (ADR 0010, section 3): trailing-slash redirects, the
// edge cache with its stale copy, and the rate limit on cache misses that an attacker could multiply. Prerendered
// pages (built at deploy time) and routes that belong to other parts of the site pass straight through.

import { defineMiddleware } from "astro:middleware";
import { clientIp } from "@news/worker-shared/http";
import type { SiteEnv } from "./data";
import { settings, siteContext } from "./server/context";
import { renderMachine } from "./server/machine";
import { cacheControl, matchRoute, POLICY } from "./server/routes";
import { serve } from "./server/serve";

export const onRequest = defineMiddleware(async (context, next) => {
  if (context.isPrerendered) return next();
  // Imported here, not at the top: the build prerenders in Node, where this module does not exist.
  const { env } = (await import("cloudflare:workers")) as unknown as { env: SiteEnv };
  const { origin, regions } = settings(env);
  const m = matchRoute(new URL(context.request.url), origin, regions);
  if (m.type === "redirect")
    return new Response(null, {
      status: 301,
      headers: { location: m.location, "cache-control": cacheControl(POLICY.redirect) },
    });
  // Feeds, sitemaps and llms.txt are built here; every other page is an Astro route.
  const render = async () =>
    m.type === "page" && (m.kind === "feed" || m.kind === "sitemap" || m.kind === "llms")
      ? renderMachine(await siteContext(env), m)
      : next();
  // A preview server (NEWS_PREVIEW=1 builds only): no cache, no limiter, everything marked noindex (src/preview.ts).
  if (typeof __NEWS_PREVIEW__ !== "undefined" && __NEWS_PREVIEW__) {
    const preview = await import("./preview");
    preview.assertLocalPreview(env);
    return preview.decorate(m.type === "page" ? await render() : await next());
  }
  if (m.type !== "page") return next();
  const ip = clientIp(context.request);
  return serve(context.request, m, {
    // No edge cache under `astro dev`: an edit should show on the next reload.
    cache: import.meta.env.DEV ? undefined : (caches as unknown as { default: Cache }).default,
    waitUntil: (p) => context.locals.cfContext?.waitUntil(p),
    now: Date.now,
    allow: env.SEARCH_RATE_LIMITER
      ? async () => (await env.SEARCH_RATE_LIMITER?.limit({ key: ip }))?.success ?? true
      : undefined,
    render,
  });
});
