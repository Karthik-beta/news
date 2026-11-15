// Link extraction for the internal link check of dist/. Pure functions over HTML text, so the unit project can
// prove the checker finds what it should (and the integration project runs it on the real build).
import { JSDOM } from "jsdom";

export const SITE_ORIGIN = "https://news.karthik.nz"; // astro.config.mjs `site`

const URL_ATTRIBUTES: [selector: string, attribute: string][] = [
  ["a[href]", "href"],
  ["area[href]", "href"],
  ["link[href]", "href"],
  ["script[src]", "src"],
  ["img[src]", "src"],
  ["iframe[src]", "src"],
  ["source[src]", "src"],
  ["video[src]", "src"],
  ["video[poster]", "poster"],
  ["audio[src]", "src"],
  ["track[src]", "src"],
  ["use[href]", "href"],
  ["form[action]", "action"],
];

export interface Ref {
  /** The page the reference is on, as a site path (`/`, `/a/`). */
  page: string;
  /** The attribute value as written. */
  raw: string;
  /** Resolved against the page's URL on the site origin. */
  url: URL;
}

export function parseHtml(html: string): Document {
  return new JSDOM(html).window.document;
}

/**
 * Every same-site URL referenced by a page: links, assets, srcset candidates, SVG <use>, form actions.
 * Relative URLs resolve against the page; `<base href>` is honoured. External, mailto:, tel:, data: and
 * javascript: URLs are left out (they are not fetched by any test).
 */
export function sameSiteRefs(doc: Document, page: string, origin = SITE_ORIGIN): Ref[] {
  const baseHref = doc.querySelector("base[href]")?.getAttribute("href");
  const base = new URL(baseHref ?? page, new URL(page, origin));
  const raws: string[] = [];
  for (const [selector, attribute] of URL_ATTRIBUTES) {
    for (const el of doc.querySelectorAll(selector)) raws.push(el.getAttribute(attribute)?.trim() ?? "");
  }
  for (const el of doc.querySelectorAll("[srcset]")) {
    for (const candidate of (el.getAttribute("srcset") ?? "").split(",")) {
      raws.push(candidate.trim().split(/\s+/)[0] ?? "");
    }
  }
  return raws
    .filter((raw) => raw !== "" && !/^(mailto|tel|data|javascript):/i.test(raw))
    .map((raw) => ({ page, raw, url: new URL(raw, base) }))
    .filter((ref) => ref.url.origin === origin);
}
