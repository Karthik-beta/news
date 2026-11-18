import { describe, expect, it } from "vitest";
import { checkCuration, type PageLookup } from "./curation";
import type { PageRecord } from "./records";
import { curationPages, makeCuration } from "./testing/fixtures";

const NOW = new Date("2026-11-12T10:00:00Z");
const lookup = (pages: PageRecord[]) =>
  new Map<string, PageLookup>(pages.map((p) => [p.page_id, { state: "live", page: p }]));
const codes = (issues: { code: string }[]) => issues.map((i) => i.code);

describe("checkCuration", () => {
  it("passes the fixture curation", () => {
    expect(checkCuration(makeCuration(), lookup(curationPages()), NOW)).toEqual([]);
  });

  it("flags figures and charts that are not the page's own", () => {
    const c = makeCuration();
    c.slots.lead.figure_id = "dp:not-there";
    c.slots.side[1] = { page_id: c.slots.side[1]?.page_id as string, chart_id: "nope" };
    expect(codes(checkCuration(c, lookup(curationPages()), NOW))).toEqual(["figure_not_on_page", "chart_not_on_page"]);
  });

  it("allows an old Tier 1 lead only when marked still leading", () => {
    const pages = curationPages();
    (pages[0] as PageRecord).published_at = "2026-10-01T00:00:00Z";
    expect(codes(checkCuration(makeCuration(), lookup(pages), NOW))).toEqual(["lead_stale"]);
    const c = makeCuration();
    c.slots.lead.still_leading = true;
    expect(checkCuration(c, lookup(pages), NOW)).toEqual([]);
    (pages[0] as PageRecord).tier = 2;
    expect(codes(checkCuration(c, lookup(pages), NOW))).toEqual(["lead_stale", "lead_stale"]);
  });

  it("counts a page about one entity at most twice", () => {
    const pages = curationPages().map((p, i) =>
      i < 3 ? { ...p, entities: [{ id: "company:apple", name: "Apple", kind: "company" as const }] } : p,
    );
    expect(codes(checkCuration(makeCuration(), lookup(pages), NOW))).toEqual(["breadth_entity"]);
  });
});
