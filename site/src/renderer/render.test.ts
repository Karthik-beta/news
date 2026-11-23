import { JSDOM } from "jsdom";
import { describe, expect, it } from "vitest";
import { MINUS } from "../format";
import { fixtureBundles } from "./fixtures/bundles/vite";
import { fixtureLibrary } from "./fixtures/library";
import { type PageBundleInput, RenderError, renderPage } from "./render";
import { RENDERER_VERSION } from "./version";

// The renderer turns fixture bundles into HTML, Markdown and chart CSV/JSON. Edge cases: an unknown reference,
// null, zero, negative, very large and very small values, a single-point series, a gap and a licence that forbids
// downloads. Fixture bundles: src/renderer/fixtures/bundles/.
const library = fixtureLibrary();
const render = (b: PageBundleInput, strict = false) => renderPage(b, { library, strict });
const dom = (html: string) => new JSDOM(html).window.document;

const apple = render(fixtureBundles["apple-fy2024"]);
const edge = render(fixtureBundles["edge-cases"]);
const appleDoc = dom(apple.html);
const edgeDoc = dom(edge.html);

const cited = (doc: Document, ref: string) => doc.querySelector(`[data-ref="${ref}"]`)?.textContent;
/** The figure inside a fixture key figure whose label is `label`. */
const figure = (doc: Document, label: string) =>
  [...doc.querySelectorAll(".fixture-figure")]
    .find((dl) => dl.querySelector("dt")?.textContent === label)
    ?.querySelector("dd.text-figure button, dd.text-figure .source-ref--missing")?.textContent;

describe("renderer: HTML", () => {
  it("writes a complete document with the head filled in by code", () => {
    expect(apple.html.startsWith("<!doctype html>")).toBe(true);
    expect(appleDoc.documentElement.lang).toBe("en-GB");
    expect(appleDoc.title).toBe("Apple's sales grew again in fiscal 2024, to $391.0bn");
    expect(appleDoc.querySelector("h1")?.textContent).toBe("Apple's sales grew again in fiscal 2024, to $391.0bn");
    expect(cited(appleDoc, "dp_aapl_net_sales_fy2024")).toBe("$391.0bn");
    const standfirst = appleDoc.querySelector(".text-standfirst")?.cloneNode(true) as Element;
    for (const p of standfirst.querySelectorAll("[popover]")) p.remove();
    expect(standfirst.textContent).toBe(
      "Total net sales rose by 2.0% on fiscal 2023, the first rise in two years. The figures come from the company's annual reports.",
    );
    expect(appleDoc.querySelectorAll("h1")).toHaveLength(1);
  });

  it("records the renderer version", () => {
    expect(apple.rendererVersion).toBe(RENDERER_VERSION);
    expect(appleDoc.querySelector('meta[name="generator"]')?.getAttribute("content")).toBe(
      `news-renderer ${RENDERER_VERSION}`,
    );
    expect(appleDoc.querySelector("article")?.getAttribute("data-renderer")).toBe(`news-renderer ${RENDERER_VERSION}`);
    expect(apple.markdown).toContain(`generator: "news-renderer ${RENDERER_VERSION}"`);
    expect(JSON.parse(apple.files["charts/chart.json"] as string).generator).toBe(`news-renderer ${RENDERER_VERSION}`);
  });

  it("gives every cited figure its source: popover with the exact value, document and link", () => {
    const trigger = appleDoc.querySelector(".page-body p button.source-ref") as HTMLElement;
    expect(trigger.textContent).toBe("$391.0bn");
    const panel = appleDoc.getElementById(trigger.getAttribute("popovertarget") as string) as HTMLElement;
    expect(panel.getAttribute("popover")).toBe("auto");
    expect(panel.querySelector(".popover__value")?.textContent).toBe("$391,035 million");
    expect(panel.textContent).toContain("Apple Inc. Form 10-K, fiscal 2024");
    expect(panel.textContent).toContain("Retrieved 1 November 2024");
    expect(panel.querySelector("a")?.getAttribute("href")).toMatch(/^https:\/\/www\.sec\.gov\//);
  });

  it("lists every cited source, with anchors the chart source lines link to", () => {
    expect(apple.sourcesUsed).toEqual(["src_aapl_10k_2022", "src_aapl_10k_2024"]);
    for (const a of appleDoc.querySelectorAll(".figure-frame__source a")) {
      expect(appleDoc.getElementById((a.getAttribute("href") as string).slice(1)), a.textContent ?? "").not.toBeNull();
    }
  });

  it("renders the chart as a server-rendered island with its data table and downloads", () => {
    const island = appleDoc.querySelector('[data-island="FixtureLineChartPlot"]') as HTMLElement;
    expect(island).not.toBeNull();
    expect(JSON.parse(island.dataset.props as string).points).toHaveLength(5);
    expect(island.querySelectorAll(".fixture-plot__marker")).toHaveLength(5);
    expect(island.querySelector('[role="img"]')?.getAttribute("aria-label")).toBe(
      "Sales grew again in fiscal 2024 after a dip. Total net sales: fiscal 2020, $274.5bn; fiscal 2021, $365.8bn; " +
        "fiscal 2022, $394.3bn; fiscal 2023, $383.3bn; fiscal 2024, $391.0bn.",
    );
    expect(appleDoc.querySelectorAll(".figure-frame__table tbody tr")).toHaveLength(5);
    const links = [...appleDoc.querySelectorAll(".figure-frame__actions a")].map((a) => a.getAttribute("href"));
    expect(links).toEqual(["charts/chart.csv", "charts/chart.json"]);
    expect(apple.hasIslands).toBe(true);
  });

  it("uses no inline styles outside island geometry, and no hard-coded colours", () => {
    for (const el of appleDoc.querySelectorAll("[style]")) expect(el.closest("[data-island]")).not.toBeNull();
    expect(appleDoc.querySelector('[fill^="#"], [stroke^="#"], [color], [bgcolor]')).toBeNull();
  });

  it("adds no script to a page without islands", () => {
    const r = render(fixtureBundles.static);
    expect(r.hasIslands).toBe(false);
    expect(r.html).not.toContain("<script");
    const withAssets = renderPage(fixtureBundles.static, {
      library,
      assets: { stylesheets: ["/site.css"], script: "/islands.js" },
    });
    expect(withAssets.html).not.toContain("<script");
    const islandPage = renderPage(fixtureBundles["apple-fy2024"], {
      library,
      assets: { stylesheets: ["/site.css"], script: "/islands.js" },
    });
    expect(islandPage.html).toContain('<script type="module" src="/islands.js"></script>');
  });

  it("writes exactly one robots meta tag from index_status, noindex by default", () => {
    const robots = (html: string) => html.match(/<meta name="robots"[^>]*>/g);
    expect(robots(render(fixtureBundles.static).html)).toEqual(['<meta name="robots" content="noindex">']);
    expect(robots(renderPage(fixtureBundles.static, { library, indexStatus: "index" }).html)).toEqual([
      '<meta name="robots" content="index, follow">',
    ]);
    expect(robots(renderPage(fixtureBundles.static, { library, indexStatus: "noindex" }).html)).toEqual([
      '<meta name="robots" content="noindex">',
    ]);
  });

  it("produces the same HTML with the fallback walker (ADR 0001, option C)", () => {
    for (const name of Object.keys(fixtureBundles) as (keyof typeof fixtureBundles)[]) {
      const b = renderPage(fixtureBundles[name], { library }).html;
      const c = renderPage(fixtureBundles[name], { library, engine: "walker" }).html;
      expect(c, name).toBe(b);
    }
  });
});

describe("renderer: edge-case values", () => {
  it("writes zero as zero, and negatives with a true minus", () => {
    expect(figure(edgeDoc, "Zero")).toBe("0%");
    expect(figure(edgeDoc, "Negative")).toBe(`${MINUS}5.7%`);
    expect(edgeDoc.querySelector(".fixture-figure__change")?.textContent).toBe("Down 1.9 points on 2022");
  });

  it("writes very large and very small values without losing or inventing digits", () => {
    expect(figure(edgeDoc, "Very large")).toBe("$123,456.8trn");
    expect(figure(edgeDoc, "Very small")).toBe("0.000000012");
    expect(figure(edgeDoc, "Rupees")).toBe("₹2.1trn");
    expect(figure(edgeDoc, "Fiscal label")).toBe("HK$82.3bn");
  });

  it("renders a null value and an unknown reference as 'No data', never zero, and reports both", () => {
    expect(figure(edgeDoc, "Not published")).toBe("No data");
    expect(figure(edgeDoc, "Unknown reference")).toBe("No data");
    expect(edgeDoc.querySelector('[data-ref="dp_edge_not_in_data"]')?.textContent).toBe("no data");
    const codes = edge.problems.map((p) => `${p.code} ${p.ref}`);
    expect(codes).toContain("no-value dp_edge_null");
    expect(codes).toContain("unknown-ref dp_edge_not_in_data");
    expect(codes).toContain("wrong-kind sr_edge_gap");
    expect(codes).toContain("unknown-ref sr_edge_not_in_data");
  });

  it("keeps fiscal labels verbatim and fills periods, context lines and quotes from data", () => {
    const text = edgeDoc.querySelector(".page-body p")?.textContent ?? "";
    expect(text).toContain("In Q4 FY26 revenue was HK$82.3bn");
    expect(text).toContain("the lowest in the series");
    expect(edgeDoc.body.textContent).toContain("“The committee decided to keep the policy rate unchanged.”");
  });

  it("draws a single-point series as one marker, and exports one row", () => {
    const single = edgeDoc.querySelector('figure[aria-labelledby="single-title"]') as Element;
    expect(single.querySelectorAll(".fixture-plot__marker")).toHaveLength(1);
    expect(edge.files["charts/single.csv"]).toBe("Period,Turnout\r\n2024,42.5\r\n");
  });

  it("leaves a gap for a missing observation: no marker, an empty CSV field, 'No data' in the table", () => {
    const gap = edgeDoc.querySelector('figure[aria-labelledby="gap-title"]') as Element;
    expect(gap.querySelectorAll(".fixture-plot__marker")).toHaveLength(2);
    expect(gap.querySelector(".fixture-plot__line")?.getAttribute("d")?.match(/M/g)).toHaveLength(2);
    expect(edge.files["charts/gap.csv"]).toBe("Period,Growth\r\n2022,1.5\r\n2023,\r\n2024,-0.4\r\n");
    expect([...gap.querySelectorAll("tbody td")].map((td) => td.textContent)).toEqual([
      "1.5%",
      "No data",
      `${MINUS}0.4%`,
    ]);
  });

  it("offers no CSV or JSON when a source's licence forbids redistribution (check C8)", () => {
    const restricted = edgeDoc.querySelector('figure[aria-labelledby="restricted-title"]') as Element;
    expect(restricted.querySelector(".figure-frame__actions a")).toBeNull();
    expect(restricted.querySelector(".figure-frame__table")).not.toBeNull();
    expect(Object.keys(edge.files).filter((f) => f.includes("restricted"))).toEqual([]);
  });

  it("renders an unknown series as the chart's empty state", () => {
    const missing = edgeDoc.querySelector('figure[aria-labelledby="nochart-title"]') as Element;
    expect(missing.querySelector(".fixture-chart-missing")?.textContent).toBe("No data");
    expect(missing.querySelector("[data-island]")).toBeNull();
  });

  it("throws in strict mode when the page has data problems", () => {
    expect(() => render(fixtureBundles["edge-cases"], true)).toThrow(RenderError);
    expect(() => render(fixtureBundles["apple-fy2024"], true)).not.toThrow();
  });
});

describe("renderer: Markdown and downloads", () => {
  it("writes index.md with front matter, the filled head, each component's Markdown and the sources", () => {
    expect(apple.markdown).toMatch(/^---\ntitle: "Apple's sales grew again in fiscal 2024, to \$391.0bn"\n/);
    expect(apple.markdown).toContain("\n# Apple's sales grew again in fiscal 2024, to $391.0bn\n");
    expect(apple.markdown).toContain("**Total net sales:** $391.0bn");
    expect(apple.markdown).toContain("| fiscal 2024 | 391035 |");
    expect(apple.markdown).toContain(
      "- [Apple Inc. Form 10-K, fiscal 2024](https://www.sec.gov/Archives/edgar/data/320193/000032019324000123/aapl-20240928.htm). Apple Inc. Public filing. Retrieved 1 November 2024.",
    );
  });

  it("exports chart CSV with exact source values and JSON with unit, currency, scale and sources", () => {
    expect(apple.files["charts/chart.csv"]).toBe(
      "Period,Total net sales\r\nfiscal 2020,274515\r\nfiscal 2021,365817\r\nfiscal 2022,394328\r\nfiscal 2023,383285\r\nfiscal 2024,391035\r\n",
    );
    const json = JSON.parse(apple.files["charts/chart.json"] as string);
    expect(json).toMatchObject({ unit: "USD", currency: "USD", scale: "millions" });
    expect(json.rows[4]).toEqual({ period: "fiscal 2024", value: "391035" });
    expect(json.sources.map((s: { id: string }) => s.id)).toEqual(["src_aapl_10k_2022", "src_aapl_10k_2024"]);
    expect(Object.keys(apple.files).sort()).toEqual([
      "charts/chart.csv",
      "charts/chart.json",
      "index.html",
      "index.md",
    ]);
  });

  it("records every reference the page used", () => {
    expect(apple.refsUsed).toContain("sr_aapl_net_sales_5y");
    expect(apple.refsUsed).toContain("dv_aapl_net_sales_growth_fy2024");
    expect(apple.refsUsed).toContain("cx_aapl_first_rise_2y");
  });
});

describe("renderer: invalid bundles", () => {
  const apple = fixtureBundles["apple-fy2024"];

  it("refuses data.json that breaks the contract", () => {
    const data = structuredClone(apple.data) as { points: Record<string, { value: unknown }> };
    (data.points.dp_aapl_net_sales_fy2024 as { value: unknown }).value = 391035;
    expect(() => render({ ...apple, data })).toThrow(/data.json breaks the bundle contract/);
  });

  it("refuses sources.json that breaks the contract", () => {
    expect(() => render({ ...apple, sources: { version: 1, sources: { bad: {} } } })).toThrow(/sources.json/);
  });

  it("refuses page.oui that does not parse, including a bare identifier instead of a quoted reference", () => {
    expect(() => render({ ...apple, oui: "root = Page([x])\nx = Nope(1)" })).toThrow(RenderError);
    const bare = 'root = Page([f])\nf = FixtureFigure("Net sales", dp_aapl_net_sales_fy2024)';
    expect(() => render({ ...apple, oui: bare })).toThrow(/write references as strings/);
  });

  it("refuses a typed number where a reference belongs", () => {
    const typed = 'root = Page([f])\nf = FixtureFigure("Net sales", "391.0")';
    expect(() => render({ ...apple, oui: typed })).toThrow(RenderError);
  });

  it("reports a broken link inside data.json", () => {
    const data = structuredClone(apple.data) as { series: Record<string, { points: { point: string }[] }> };
    (data.series.sr_aapl_net_sales_5y?.points[0] as { point: string }).point = "dp_gone";
    const r = render({ ...apple, data });
    expect(r.problems.map((p) => p.code)).toContain("broken-link");
  });
});
