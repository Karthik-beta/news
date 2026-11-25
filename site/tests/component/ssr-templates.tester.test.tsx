// Tester's checks for the SSR page templates of #14 (src/views/Views.tsx over src/server/views.ts): every template and
// every fixture curation rendered to the markup the Worker sends, then axe (WCAG 2.2 AA, both themes), the heading
// outline, landmark names and the states each template must show. Colour contrast and geometry are in the browser
// (tests/e2e/ssr-templates.tester.spec.ts); the loaders' reads and rules are in src/server/*.test.ts.

import type { SearchShard } from "@news/worker-shared/search";
import { createElement } from "react";
import { describe, expect, it } from "vitest";
import { fixtureBundles } from "../../src/renderer/fixtures/bundles/vite";
import { staticMarkup } from "../../src/renderer/markup";
import { matchRoute } from "../../src/server/routes";
import { type Body, loadView, notFoundView } from "../../src/server/views";
import { Views } from "../../src/views/Views";
import { expectNoAxeViolations, renderComponent } from "../support/render";
import { ctxOf, FIXTURE_ITEMS, ORIGIN, REGIONS, tableOf } from "../support/site-data";

const APPLE = "/2025/01/apple-fy2024-results";
const FED = "/2025/01/fed-december-2024";
const FILES = {
  [`${APPLE} data.json`]: fixtureBundles["apple-fy2024"].data,
  [`${APPLE} sources.json`]: fixtureBundles["apple-fy2024"].sources,
  [`${FED} data.json`]: fixtureBundles.charts.data,
  [`${FED} sources.json`]: fixtureBundles.charts.sources,
  [`${FED} page.oui`]: fixtureBundles.charts.oui,
};

const ROW = {
  page_id: "us-cpi-december-2024",
  path: "/2025/01/us-cpi-december-2024",
  lang: "en",
  title: "American consumer prices rose again in December",
  summary: "The index climbed in every month of the year.",
  categories: " economy economy/inflation ",
  region: "north-america",
  page_type: "macro",
  published_at: "2025-01-15T13:00:00.000Z",
};

/** A D1 shard that answers `rows`, or throws (the daily limit). */
const shardOf = (rows: unknown[] | Error): SearchShard => ({
  year: 2025,
  db: {
    prepare: () => ({
      bind: () => ({
        all: async () => {
          if (rows instanceof Error) throw rows;
          return { results: rows };
        },
      }),
    }),
  } as unknown as SearchShard["db"],
});

async function bodyOf(path: string, set = "normal", rows: unknown[] | Error = [ROW]): Promise<Body> {
  const ctx = ctxOf(tableOf(FIXTURE_ITEMS[set]), { files: FILES });
  ctx.data = { ...ctx.data, shards: [shardOf(rows)] };
  const m = matchRoute(new URL(`${ORIGIN}${path}`), ORIGIN, REGIONS);
  const v = m.type === "page" ? await loadView(ctx, m) : await notFoundView(ctx);
  if (v.type !== "view") throw new Error(`${path} redirects`);
  return v.body;
}

/** Heading levels in document order; the outline must start at 1 and never skip a level going down. */
const outline = (root: Element) => [...root.querySelectorAll("h1,h2,h3,h4,h5,h6")].map((h) => Number(h.tagName[1]));
function expectOutline(root: Element) {
  const levels = outline(root);
  expect(levels[0], "the first heading is the h1").toBe(1);
  expect(
    levels.filter((l) => l === 1),
    "one h1",
  ).toHaveLength(1);
  for (let i = 1; i < levels.length; i++)
    expect(levels[i] as number, `heading ${i} (h${levels[i]}) after h${levels[i - 1]}`).toBeLessThanOrEqual(
      (levels[i - 1] as number) + 1,
    );
}

/** Every <nav> and <section> with a name has a different one (axe checks landmarks; this also covers sections). */
function expectUniqueRegionNames(root: Element) {
  const names = [...root.querySelectorAll("nav[aria-label], section[aria-label]")].map(
    (el) => `${el.tagName}:${el.getAttribute("aria-label")}`,
  );
  expect(names.length - new Set(names).size, `repeated names: ${names.join(", ")}`).toBe(0);
}

const CASES: [name: string, path: string, set?: string, rows?: unknown[] | Error][] = [
  ["home, normal curation", "/"],
  ["home, withdrawn lead", "/", "withdrawn-lead"],
  ["home, region set", "/", "region"],
  ["home, empty Latest", "/", "empty-latest"],
  ["region view, region set", "/region/india", "region"],
  ["region view, normal", "/region/europe"],
  ["Latest, first page", "/latest"],
  ["category hub", "/economy"],
  ["sub-category hub", "/economy/inflation"],
  ["category hub, region filter", "/economy?region=north-america"],
  ["category hub, region with no pages", "/finance?region=australia"],
  ["entity hub (company, strip)", "/entity/company%3Aapple"],
  ["entity hub (country)", "/entity/country%3AUS"],
  ["search, form only", "/search"],
  ["search, results", "/search?q=inflation&region=north-america"],
  ["search, empty", "/search?q=zzzz", "normal", []],
  ["search, too many words", "/search?q=a+b+c+d+e+f+g+h+i"],
  ["search, daily limit", "/search?q=inflation", "normal", new Error("D1_ERROR: daily limit")],
  ["404", "/no-such-section"],
];

describe("SSR templates: accessible markup in both themes", () => {
  for (const [name, path, set, rows] of CASES) {
    for (const theme of ["light", "dark"] as const) {
      it(`${name} (${path}, ${set ?? "normal"}) in ${theme}`, async () => {
        const body = await bodyOf(path, set, rows);
        const { container, unmount } = renderComponent(createElement(Views, { body }), { theme });
        try {
          await expectNoAxeViolations(container);
          if (body.type !== "notfound") expectOutline(container);
          expectUniqueRegionNames(container);
          // Every link goes somewhere (a site path, or a source's https address), and no "undefined" leaks into text.
          for (const a of container.querySelectorAll("a"))
            expect(a.getAttribute("href"), a.outerHTML).toMatch(/^(\/(?!\/)|https:\/\/\S)\S*$/);
          expect(container.textContent).not.toMatch(/undefined|NaN|\[object/);
        } finally {
          unmount();
        }
      });
    }
  }
});

describe("SSR templates: the states each template shows", () => {
  const html = async (path: string, set?: string, rows?: unknown[] | Error) =>
    staticMarkup(createElement(Views, { body: await bodyOf(path, set, rows) }));

  it("home: lead, side stories, Latest and sections; Latest says how many and links to /latest", async () => {
    const body = await bodyOf("/");
    if (body.type !== "home") throw new Error(body.type);
    expect(body.model.lead).toBeDefined();
    expect(body.model.side.length).toBeGreaterThan(0);
    expect(body.model.sections.length).toBeGreaterThan(0);
    expect(body.model.latest.length).toBeLessThanOrEqual(10);
    expect(await html("/")).toContain('href="/latest"');
  });

  it("home with an empty Latest: no Latest list and no empty-state message (the slots still show)", async () => {
    const body = await bodyOf("/", "empty-latest");
    if (body.type !== "home") throw new Error(body.type);
    expect(body.model.latest).toEqual([]);
    const out = await html("/", "empty-latest");
    expect(out).not.toContain("latest-list");
    expect(out).not.toContain("There are no pages yet.");
  });

  it("home with a withdrawn lead: the withdrawn page's title appears nowhere", async () => {
    expect(await html("/", "withdrawn-lead")).not.toContain("Apple reports its results for the 2024 fiscal year");
  });

  it("Latest pages at 20 with an Older link, and the Older page is the rest", async () => {
    const first = await bodyOf("/latest");
    if (first.type !== "hub") throw new Error(first.type);
    expect(first.entries).toHaveLength(20);
    expect(first.olderHref).toMatch(/^\/latest\?after=/);
    const older = await bodyOf(first.olderHref as string);
    if (older.type !== "hub") throw new Error(older.type);
    expect(older.entries).toHaveLength(FIXTURE_ITEMS.normal?.filter((i) => i.PK === "HUB#latest").length - 20);
    expect(older.olderHref).toBeUndefined();
    const ids = new Set(first.entries.map((e) => e.page_id));
    expect(older.entries.some((e) => ids.has(e.page_id))).toBe(false);
  });

  it("a region with no pages in a hub says so and offers all regions", async () => {
    const out = await html("/finance?region=australia");
    expect(out).toContain("There are no pages in this region yet.");
    expect(out).toContain('href="/finance"');
  });

  it("the daily-limit search page has no results and points to Latest", async () => {
    const out = await html("/search?q=inflation", "normal", new Error("D1_ERROR"));
    expect(out).toContain("00:00 UTC");
    expect(out).not.toContain('role="status"');
  });

  // SSR-1 (design/components.md "Empty state"): with a filter set, the way on is to clear the filters.
  it("an empty search with filters offers to clear them; without filters, Latest", async () => {
    const filtered = await html("/search?q=zzzz&region=india&type=macro", "normal", []);
    expect(filtered).toContain('<a href="/search?q=zzzz">Clear the filters</a>');
    const plain = await html("/search?q=zzzz", "normal", []);
    expect(plain).toContain('<a href="/latest">See the latest pages</a>');
    expect(plain).not.toContain("Clear the filters");
  });
});
