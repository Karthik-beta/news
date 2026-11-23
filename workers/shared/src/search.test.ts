// The search parameters the API and the site's page share: parsing and the normalised cache key.
import { describe, expect, it } from "vitest";
import { parseSearchParams, searchCacheKey } from "./search";

const parse = (qs: string, optionalQuery = false) => parseSearchParams(new URLSearchParams(qs), { optionalQuery });

describe("parseSearchParams", () => {
  it("accepts the contract's filters", () => {
    expect(
      parse("q=inflation&category=economy/inflation&region=india&type=macro&from=2025-01-01&to=2025-12-31&limit=20"),
    ).toEqual({
      ok: true,
      params: {
        query: "inflation",
        category: "economy/inflation",
        region: "india",
        page_type: "macro",
        from: "2025-01-01",
        to: "2025-12-31",
        limit: 20,
      },
    });
  });

  it("needs q, except for the site's empty search form", () => {
    expect(parse("")).toMatchObject({ ok: false });
    expect(parse("", true)).toEqual({ ok: true, params: { query: "" } });
  });

  it.each([
    "q=a&region=North America",
    "q=a&category=Economy",
    "q=a&type=gossip",
    "q=a&from=2025-02-30",
    "q=a&to=yesterday",
    "q=a&limit=0",
    "q=a&limit=51",
    `q=${"a".repeat(201)}`,
  ])("refuses %s", (qs) => {
    expect(parse(qs)).toMatchObject({ ok: false });
  });
});

describe("searchCacheKey", () => {
  it("is the same for the same words in any case and spacing, and any filter order", () => {
    const key = (qs: string) => {
      const r = parse(qs);
      if (!r.ok) throw new Error(r.message);
      return searchCacheKey(r.params);
    };
    expect(key("q=Inflation++US&region=india&type=macro")).toBe(key("type=macro&q=inflation us!&region=india"));
    expect(key("q=inflation")).not.toBe(key("q=inflation&region=india"));
    expect(key("q=inflation")).not.toBe(key("q=us inflation"));
  });
});
