// Every configured value the reader-facing static pages show (issue #14). Page copy in content/pages/*.md never
// types one of these: it writes a token such as {contact.email}, and src/static/content.ts fills it in from here at
// build time. A test fails if a value below appears literally in the copy.
//
// Karthik fills the values marked UNSET or example.invalid (and sets NEWS_API_ORIGIN at build time). Values copied
// from code or contracts are checked against their source in tests/unit/static-config.test.ts, so they cannot drift.
import report from "../../../contracts/cloud/report.schema.json";

/** What a value shows until Karthik sets it. `unsetKeys()` lists every key still holding a placeholder. */
export const UNSET = "[not set: edit site/src/config/site.ts]";
/** Reserved TLD: nothing can be sent to it, so a placeholder address or origin never reaches a stranger. */
export const PLACEHOLDER_HOST = "example.invalid";

const origin = "https://news.karthik.nz"; // astro.config.mjs `site`; a test keeps them equal
const apiOrigin = process.env.NEWS_API_ORIGIN ?? `https://api.${PLACEHOLDER_HOST}`; // set NEWS_API_ORIGIN at build

export const siteConfig = {
  site: { origin },
  contact: { email: `contact@${PLACEHOLDER_HOST}` },
  operator: { legalName: UNSET, jurisdiction: UNSET },
  licence: {
    name: "CC BY 4.0",
    url: "https://creativecommons.org/licenses/by/4.0/",
  },
  /** Where the report form posts (the intake is #6's `POST /v1/reports`) and its limits (the contract's). */
  report: {
    postUrl: `${apiOrigin}/v1/reports`,
    text: { minLength: report.$defs.text.minLength, maxLength: report.$defs.text.maxLength },
    where: { maxLength: report.$defs.where.maxLength },
  },
  /** workers/api/src/reports.ts RETENTION_SECONDS (ADR 0006). */
  reports: { retentionDays: 180 },
  mcp: {
    url: `${origin}/mcp`, // ADR 0010 zone route `news.karthik.nz/mcp*`
    ratePerMinute: 30, // workers/mcp/wrangler.jsonc
    searchMaxWords: 8, // workers/shared/src/search.ts MAX_TERMS
    entityPageLimit: 20, // contracts/cloud/mcp-tools.schema.json
  },
  /** Technical Design: NEWS_RAW_RETAIN_DAYS_DATA and NEWS_RAW_RETAIN_DAYS_DOCS are still pending. */
  raw: { retainDaysData: UNSET, retainDaysDocs: UNSET },
  fetcher: {
    version: UNSET,
    userAgent: `News/${UNSET} (+${origin}/data)`,
  },
};

export type SiteConfig = typeof siteConfig;
type Leaf = string | number;

/** Leaf values by dotted path: ["contact.email", "..."]. */
export function configLeaves(config: object = siteConfig, prefix = ""): [string, Leaf][] {
  return Object.entries(config).flatMap(([k, v]): [string, Leaf][] =>
    typeof v === "object" && v !== null ? configLeaves(v, `${prefix}${k}.`) : [[`${prefix}${k}`, v as Leaf]],
  );
}

/** Keys that still hold a placeholder, for Karthik to fill. */
export const unsetKeys = (): string[] =>
  configLeaves()
    .filter(([, v]) => typeof v === "string" && (v.includes(UNSET) || v.includes(PLACEHOLDER_HOST)))
    .map(([k]) => k);
