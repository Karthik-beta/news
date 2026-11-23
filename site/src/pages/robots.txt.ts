// robots.txt (ADR 0010, section 4): every agent may read the site, except the search page; the sitemap is named.
// Prerendered, so it is a static asset: Cloudflare serves it without running the Worker.
import type { APIRoute } from "astro";
import { siteConfig } from "../config/site";

export const GET: APIRoute = () =>
  new Response(`User-agent: *\nAllow: /\nDisallow: /search\n\nSitemap: ${siteConfig.site.origin}/sitemap.xml\n`, {
    headers: { "content-type": "text/plain; charset=utf-8" },
  });
