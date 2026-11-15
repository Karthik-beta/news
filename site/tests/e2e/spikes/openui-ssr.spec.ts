// OpenUI server-rendering spike (ADR 0001): the fixture page is complete HTML before any JavaScript, static
// parts never hydrate (0 KB JS for them), and the chart island hydrates with no hydration warnings.
// B renders through react-dom/server with our own island hydrator (the chosen approach).
// A (Astro native islands, rejected) is built and tested only with OPENUI_SPIKE_A=1. Code: site/spikes/openui-ssr/.
// The spike routes are built only with OPENUI_SPIKE=1 (re-running the ADR measurements); otherwise this file skips.
// The production renderer has its own browser test: tests/e2e/renderer/fixture-pages.spec.ts.
import { expect, gotoPage, test } from "../fixtures";

const PAGES: Record<string, string> =
  process.env.OPENUI_SPIKE === "1" ? { "B (React server rendering)": "/fixtures/openui-ssr/react-ssr/" } : {};
if (process.env.OPENUI_SPIKE_A === "1") PAGES["A (Astro islands)"] = "/fixtures/openui-ssr/astro-islands/";

const STATIC = ["h1.oui-heading", "p.oui-paragraph", ".oui-figure"];

/** True when React has hydrated (attached a fiber to) the element. */
const isReactManaged = (el: Element) => Object.keys(el).some((k) => k.startsWith("__reactFiber$"));

for (const [name, path] of Object.entries(PAGES)) {
  test(`OpenUI fixture ${name}: static HTML, static parts ship no JS, chart island hydrates`, {
    tag: "@smoke",
  }, async ({ page, browser }) => {
    // 1. Without JavaScript the page is complete: headline, paragraph, key figure and the drawn chart.
    const noJs = await browser.newContext({ javaScriptEnabled: false });
    const raw = await noJs.newPage();
    await raw.goto(path);
    for (const sel of STATIC) await expect(raw.locator(sel)).toBeVisible();
    await expect(raw.locator(".oui-chart svg path.oui-chart-line")).toBeVisible();
    await expect(raw.locator(".oui-chart-point")).toHaveCount(5);
    await noJs.close();

    // 2. With JavaScript: watch every script and every warning while the page loads and hydrates.
    const hydrationWarnings: string[] = [];
    page.on("console", (msg) => {
      if (/hydrat|did not match|mismatch|recoverable/i.test(msg.text())) hydrationWarnings.push(msg.text());
    });
    const scripts: string[] = [];
    page.on("request", (req) => {
      if (req.resourceType() === "script") scripts.push(new URL(req.url()).pathname);
    });
    await gotoPage(page, path);

    // 3. The island hydrates: React marks it live, and the keyboard tooltip works (server HTML alone cannot).
    const chart = page.locator("figure.oui-chart");
    await expect(chart).toHaveAttribute("data-hydrated", "true");
    const plot = page.locator(".oui-chart-plot");
    await plot.focus();
    await page.keyboard.press("ArrowRight");
    await expect(page.locator(".oui-chart-tooltip")).toHaveText("274.5 FY2020");
    await page.keyboard.press("Escape");
    await expect(page.locator(".oui-chart-tooltip")).toHaveCount(0);

    // 4. Static parts never hydrate: React manages only the chart, not the headline, text or key figure.
    expect(await chart.evaluate(isReactManaged)).toBe(true);
    for (const sel of STATIC) expect(await page.locator(sel).evaluate(isReactManaged), sel).toBe(false);
    // Every script is the island's (served by the build from /_astro/); none is inline page code for static parts.
    expect(scripts.length).toBeGreaterThan(0);
    for (const src of scripts) expect(src).toMatch(/^\/_astro\//);

    expect(hydrationWarnings, "hydration warnings in the console").toEqual([]);
  });
}
