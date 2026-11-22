// Every configured value the reader-facing static pages show (issue #14). Page copy in content/pages/*.md never
// types one of these: it writes a token such as {contact.email}, and src/static/content.ts fills it in from here at
// build time. A test fails if a value below appears literally in the copy.
//
// Karthik fills the values marked UNSET or example.invalid; a production build (NEWS_ENV=production) fails until he
// has. Values copied from code or contracts are checked against their source in tests/unit/static-config.test.ts,
// so they cannot drift.
import report from "../../../contracts/cloud/report.schema.json";

/** What a value shows until Karthik sets it. `unsetKeys()` lists every key still holding a placeholder. */
export const UNSET = "[not set: edit site/src/config/site.ts]";
/** Reserved TLD: nothing can be sent to it, so a placeholder address or origin never reaches a stranger. */
export const PLACEHOLDER_HOST = "example.invalid";

const origin = "https://news.karthik.nz"; // astro.config.mjs `site`; a test keeps them equal
const cliVersion = "0.0.1";

export const siteConfig = {
  site: { origin },
  contact: { email: `contact@${PLACEHOLDER_HOST}` },
  operator: { legalName: UNSET, jurisdiction: UNSET },
  licence: {
    name: "CC BY 4.0",
    url: "https://creativecommons.org/licenses/by/4.0/",
  },
  /** The report contract's limits. */
  report: {
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
  /** cli/Cargo.toml `version` and the format in news-wire runner.rs `user_agent`; static-config.test.ts checks both. */
  fetcher: {
    version: cliVersion,
    userAgent: `News/${cliVersion} (+${origin}/data)`,
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

/** Throws when a production build would publish a placeholder. */
export function assertProductionReady(env = process.env.NEWS_ENV): void {
  const unset = unsetKeys();
  if (env === "production" && unset.length) {
    throw new Error(`site/src/config/site.ts still has placeholders: ${unset.join(", ")}`);
  }
}
assertProductionReady();
