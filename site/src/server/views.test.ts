// The loaders: what is read for each page (the budget of ADR 0010 section 2) and what a slot's figure or chart reads.

import { createElement } from "react";
import { describe, expect, it } from "vitest";
import { ctxOf, FIXTURE_ITEMS, ORIGIN, REGIONS, tableOf } from "../../tests/support/site-data";
import { fixtureBundles } from "../renderer/fixtures/bundles/vite";
import { staticMarkup } from "../renderer/markup";
import { Views } from "../views/Views";
import { matchRoute } from "./routes";
import { loadView } from "./views";

const APPLE = "/2025/01/apple-fy2024-results";
const FED = "/2025/01/fed-december-2024";
const files = {
  [`${APPLE} data.json`]: fixtureBundles["apple-fy2024"].data,
  [`${APPLE} sources.json`]: fixtureBundles["apple-fy2024"].sources,
  [`${FED} data.json`]: fixtureBundles.charts.data,
  [`${FED} sources.json`]: fixtureBundles.charts.sources,
  [`${FED} page.oui`]: fixtureBundles.charts.oui,
};

const view = async (path: string, set = "normal", withFiles: Record<string, unknown> = files) => {
  const table = tableOf(FIXTURE_ITEMS[set]);
  const ctx = ctxOf(table, { files: withFiles });
  const m = matchRoute(new URL(`${ORIGIN}${path}`), ORIGIN, REGIONS);
  if (m.type !== "page") throw new Error("not a page");
  const v = await loadView(ctx, m);
  if (v.type !== "view") throw new Error("redirect");
  return { table, ctx, v, html: staticMarkup(createElement(Views, { body: v.body })) };
};

describe("the home page", () => {
  it("reads at most 12 DynamoDB calls on a miss, and R2 only for the slots that show a figure or chart", async () => {
    const reads: string[] = [];
    const { table } = await view(
      "/",
      "normal",
      new Proxy(files, { get: (t, k) => (reads.push(String(k)), Reflect.get(t, k)) }),
    );
    expect(table.calls.length).toBeLessThanOrEqual(12);
    // The lead's figure (data and sources) and the side story's chart (data, sources and its page.oui).
    expect(new Set(reads.map((r) => r.split(" ")[0]))).toEqual(new Set([APPLE, FED]));
  });

  it("a withdrawn lead is back-filled within the same budget", async () => {
    const { table, html } = await view("/", "withdrawn-lead");
    expect(table.calls.length).toBeLessThanOrEqual(12);
    expect(html).not.toContain("Apple reports its results");
    expect(html).toContain("European lenders open the earnings season");
  });

  it("shows a figure and a mini chart read from the slot pages' own data, never typed", async () => {
    const { html } = await view("/");
    expect(html).toContain('data-ref="dp_aapl_net_sales_fy2024"');
    expect(html).toContain('data-chart="path"');
    expect(html).toContain("Federal funds target range, upper limit");
  });

  it("a slot whose data cannot be read keeps its story and loses its figure", async () => {
    const { html } = await view("/", "normal", {});
    expect(html).toContain("Apple reports its results for the 2024 fiscal year");
    expect(html).toContain("The Federal Reserve cuts its policy rate again");
    expect(html).not.toContain("slot-figure");
  });

  it("the date line, the region filter to /region/<code>, and the Latest list link", async () => {
    const { html } = await view("/");
    expect(html).toContain("Wednesday 15 January 2025");
    expect(html).toContain('href="/region/india"');
    expect(html).toContain('href="/latest"');
    expect(html).toContain('aria-current="page">All regions');
  });

  it("with no curation yet, shows the newest pages instead of an error", async () => {
    const items = (FIXTURE_ITEMS.normal ?? []).filter((i) => i.PK !== "HOME#CURATION");
    const table = tableOf(items);
    const ctx = ctxOf(table);
    const m = matchRoute(new URL(`${ORIGIN}/`), ORIGIN, REGIONS);
    const v = m.type === "page" ? await loadView(ctx, m) : undefined;
    expect(v?.type === "view" && v.body.type === "home" && v.body.model.lead?.backfilled).toBe(true);
  });

  it("an empty site says so", async () => {
    const table = tableOf([]);
    const m = matchRoute(new URL(`${ORIGIN}/`), ORIGIN, REGIONS);
    const v = m.type === "page" ? await loadView(ctxOf(table), m) : undefined;
    if (v?.type !== "view") throw new Error("no view");
    expect(staticMarkup(createElement(Views, { body: v.body }))).toContain("There are no pages yet.");
  });
});

describe("region views", () => {
  it("show the region's curated stories first, then its latest pages, and link to older ones", async () => {
    const { v, html, table } = await view("/region/india", "region");
    expect(table.calls.length).toBeLessThanOrEqual(8);
    expect(v.body.type === "region" && v.body.olderHref).toMatch(/^\/region\/india\?after=/);
    expect(html).toContain('aria-current="page">India');
    const order = [
      "economy grows briskly",
      "expect a stronger monsoon",
      "becomes a bigger market for premium phones",
    ].map((t) => html.indexOf(t));
    expect(order.every((i) => i > 0)).toBe(true);
    expect(order).toEqual([...order].sort((a, b) => a - b));
  });

  it("the older pages are an ordinary hub of 20", async () => {
    const first = await view("/region/india", "region");
    const href = first.v.body.type === "region" ? first.v.body.olderHref : undefined;
    const older = await view(href as string, "region");
    expect(older.v.type === "view" && older.v.chrome.robots).toBe("noindex, follow");
    expect(older.v.body.type === "hub" && older.v.body.entries.length).toBeGreaterThan(0);
  });
});

describe("entity hubs", () => {
  it("show up to four of the newest page's first figures, read from its data.json", async () => {
    const { html, v } = await view("/entity/company%3Aapple");
    expect(v.body.type === "hub" && v.body.strip?.figureIds).toHaveLength(4);
    expect(html).toContain("Latest figures");
    expect(html).toContain("2025");
  });

  it("without the data (or over 256 KB), no strip", async () => {
    const { html } = await view("/entity/company%3Aapple", "normal", {});
    expect(html).not.toContain("Latest figures");
  });
});
