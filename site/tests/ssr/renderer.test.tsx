// SSR tests in Bun (`bun test tests/ssr`): the renderer runs in plain Bun, the way the page runner and the warm
// render server (#17) call it, with no Vite or Astro in between. Checks that every fixture bundle renders, that
// islands are React's own server output (so hydration matches), that both engines agree, and the render-time
// budget on this machine.
import { describe, expect, test } from "bun:test";
import { createElement } from "react";
import { renderToString } from "react-dom/server";
import { FIXTURE_BUNDLES } from "../../src/renderer/fixtures/bundles/assemble";
import { readFixtureBundles } from "../../src/renderer/fixtures/bundles/fs";
import FixtureLineChartPlot from "../../src/renderer/fixtures/components/FixtureLineChart/FixtureLineChartPlot.client";
import { fixtureLibrary } from "../../src/renderer/fixtures/library";
import { renderPage } from "../../src/renderer/render";

const bundles = readFixtureBundles();
const library = fixtureLibrary();

describe("renderer under Bun", () => {
  test("runs in Bun itself", () => {
    expect(typeof Bun.version).toBe("string");
  });

  for (const name of FIXTURE_BUNDLES) {
    test(`renders ${name} to a complete document, with both engines byte-identical`, () => {
      const b = renderPage(bundles[name], { library });
      const c = renderPage(bundles[name], { library, engine: "walker" });
      expect(b.html.startsWith("<!doctype html>")).toBe(true);
      expect(b.html).toContain("</article></main></body></html>");
      expect(c.html).toBe(b.html);
    });
  }

  test("an island's markup is exactly React's renderToString of the same component and props", () => {
    const { article } = renderPage(bundles["apple-fy2024"], { library });
    const m =
      /<div class="island" data-island="FixtureLineChartPlot" data-island-prefix="([^"]+)" data-props="([^"]+)">/.exec(
        article,
      );
    expect(m).not.toBeNull();
    const [whole, prefix, propsAttr] = m as unknown as [string, string, string];
    const props = JSON.parse(propsAttr.replaceAll("&quot;", '"').replaceAll("&amp;", "&").replaceAll("&#x27;", "'"));
    const expected = renderToString(createElement(FixtureLineChartPlot, props), { identifierPrefix: prefix });
    expect(article).toContain(`${whole}${expected}</div>`);
  });

  test("static parts carry no hydration markers and no scripts", () => {
    const { html } = renderPage(bundles.static, { library });
    expect(html).not.toContain("<script");
    expect(html).not.toContain("data-island");
    expect(html).not.toMatch(/<!--\$|<template/); // no Suspense or streaming markers
  });

  test("renders within budget: p95 under 50 ms over 50 renders of the island page", () => {
    const times: number[] = [];
    for (let i = 0; i < 55; i++) {
      const t = performance.now();
      renderPage(bundles["apple-fy2024"], { library });
      if (i >= 5) times.push(performance.now() - t);
    }
    times.sort((a, b) => a - b);
    const p95 = times[Math.ceil(times.length * 0.95) - 1] as number;
    console.log(`render p50 ${times[24]?.toFixed(1)} ms, p95 ${p95.toFixed(1)} ms`);
    expect(p95).toBeLessThan(50);
  });
});
