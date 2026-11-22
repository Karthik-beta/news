import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { configLeaves, PLACEHOLDER_HOST, siteConfig, UNSET, unsetKeys } from "../../src/config/site";
import { fillTokens, glossaryItems, loadDoc, parseDoc, rawCopy } from "../../src/static/content";
import { stripComments } from "../../src/static/markdown";
import { repoRoot, siteRoot } from "../support/tokens";

const read = (...p: string[]) => readFileSync(join(repoRoot, ...p), "utf8");
const copy = rawCopy();
const PAGES = Object.keys(copy);

describe("static-page config", () => {
  it("has the copy files the pages need", () => {
    expect(PAGES.sort()).toEqual(
      [
        "404",
        "about",
        "contact",
        "corrections",
        "data-access",
        "glossary",
        "legal",
        "methodology",
        "privacy",
        "report-an-error",
      ].sort(),
    );
  });

  it("matches the code and contracts it is copied from, so a page cannot drift from the system", () => {
    expect(read("workers/api/src/reports.ts")).toContain(
      `RETENTION_SECONDS = ${siteConfig.reports.retentionDays} * 24 * 3600`,
    );
    expect(read("workers/mcp/wrangler.jsonc")).toMatch(
      new RegExp(`"limit": ${siteConfig.mcp.ratePerMinute}, "period": 60`),
    );
    expect(read("workers/shared/src/search.ts")).toContain(`MAX_TERMS = ${siteConfig.mcp.searchMaxWords};`);
    const tools = read("contracts/cloud/mcp-tools.schema.json");
    expect(tools).toContain(`"maximum": ${siteConfig.mcp.entityPageLimit}`);
    expect(readFileSync(join(siteRoot, "astro.config.mjs"), "utf8")).toContain(`site: "${siteConfig.site.origin}"`);
    const report = JSON.parse(read("contracts/cloud/report.schema.json")).$defs;
    expect(siteConfig.report.text).toEqual({ minLength: report.text.minLength, maxLength: report.text.maxLength });
    expect(siteConfig.report.where.maxLength).toBe(report.where.maxLength);
  });

  it("lists the placeholder keys Karthik must fill", () => {
    expect(unsetKeys().sort()).toEqual(
      [
        "contact.email",
        "fetcher.userAgent",
        "fetcher.version",
        "operator.jurisdiction",
        "operator.legalName",
        "raw.retainDaysData",
        "raw.retainDaysDocs",
        "report.postUrl",
      ].sort(),
    );
    expect(UNSET).not.toContain(PLACEHOLDER_HOST);
  });
});

describe("static-page copy", () => {
  it.each(PAGES)("%s: every {token} has a value, and the front matter is complete", (name) => {
    const doc = loadDoc(name);
    expect(doc.title).not.toBe("");
    expect(doc.blocks.length).toBeGreaterThan(0);
    expect(JSON.stringify(doc.blocks)).not.toMatch(/\{[a-z]+\.[A-Za-z.]+\}/);
  });

  it("fails the build on a token with no value", () => {
    expect(() => fillTokens("a {no.such.key} b")).toThrow(/no\.such\.key/);
    expect(fillTokens("<!-- {no.such.key} --> {reports.retentionDays} days")).toBe(" 180 days");
  });

  // The rule: a configured value is written once, in config, and the copy and the pages use a token or a prop.
  describe("never hard-codes a configured value", () => {
    const sources = [
      ...PAGES.map((n) => [`content/pages/${n}.md`, stripComments(copy[n] as string)] as const),
      ...["methodology", "about", "glossary", "privacy", "legal", "contact", "report", "data", "corrections"].map(
        (n) => [`src/pages/${n}.astro`, readFileSync(join(siteRoot, "src/pages", `${n}.astro`), "utf8")] as const,
      ),
      ...["StaticPage.astro", "StaticDoc.astro"].map(
        (n) => [`src/layouts/${n}`, readFileSync(join(siteRoot, "src/layouts", n), "utf8")] as const,
      ),
    ];
    // Placeholders are not values yet; a bare number is matched as a whole figure, not inside a date or version.
    const values = configLeaves().filter(
      ([, v]) => !(typeof v === "string" && (v.includes(UNSET) || v.includes(PLACEHOLDER_HOST))),
    );
    const found = (text: string, v: string | number) =>
      typeof v === "number"
        ? new RegExp(`(?<![\\w.\\-/])${v}(?![\\w.\\-])`).test(text)
        : // The origin may appear in the layout, which builds the canonical link from config; never in copy.
          text.includes(v);

    it.each(sources)("%s", (_file, text) => {
      const leaked = values.filter(
        ([key, v]) => found(text, v) && !(key === "site.origin" && _file.startsWith("src/")),
      );
      expect(leaked.map(([k]) => k)).toEqual([]);
    });

    it("catches a planted value", () => {
      expect(found("Write to CC BY 4.0 now", "CC BY 4.0")).toBe(true);
      expect(found("kept 180 days", 180)).toBe(true);
      expect(found("revision 2025-11-30 and 1180 days", 180)).toBe(false);
    });
  });

  it("links every glossary term to an entry the glossary has", () => {
    const slugs = new Set(glossaryItems().map((g) => g.slug));
    for (const name of PAGES) {
      for (const m of (copy[name] as string).matchAll(/\]\(glossary:([a-z0-9-]+)\)/g)) {
        expect(slugs.has(m[1] as string), `${name}.md links glossary:${m[1]}`).toBe(true);
      }
    }
  });

  it("reads the glossary A to Z, in order, with unique slugs", () => {
    const items = glossaryItems();
    expect(items.length).toBeGreaterThan(10);
    expect(items.map((i) => i.letter)).toEqual([...items.map((i) => i.letter)].sort());
    expect(new Set(items.map((i) => i.slug)).size).toBe(items.length);
    expect(items.find((i) => i.slug === "calculated-by-news-from")?.term).toBe("Calculated by News from …");
  });

  it("rejects copy with no front matter", () => {
    expect(() => parseDoc("## Hello")).toThrow(/front matter/);
  });
});
