// Screenshot baselines for Charts II, Charts III and ChoroplethMap (#71): each fixture page's <main>, after every
// visible island has hydrated, against the committed PNG in tests/e2e/__screenshots__/<project>/. Compared only in the
// Playwright Docker image (`bun run test:visual`; site/TESTING.md, "Screenshots"). Tagged @smoke too, so the CI
// screenshot job compares the smoke projects (Chromium, 360 and 1440, light and dark) on every PR.
import { expect, gotoPage, test } from "../fixtures";

const PAGES = [
  "Waterfall",
  "Sankey",
  "StackedBar",
  "SegmentedBar",
  "DotPlot",
  "Slope",
  "SmallMultiples",
  "Heatmap",
  "Histogram",
  "ChoroplethMap",
].flatMap((n) => [`${n}/default`, `${n}/edge-cases`]);

/** Projects with committed baselines (site/TESTING.md, "Screenshots"): Chromium, light and dark at 360 and 1440 (the
 * smoke screens, which share them), and light at 768 and 980, one screen per layout breakpoint. */
const hasBaselines = (project: string) =>
  /^smoke-/.test(project) || /^chromium-((light|dark)-(360|1440)|light-(768|980))$/.test(project);

for (const p of PAGES) {
  test(`${p}: matches its screenshot baseline`, { tag: ["@visual", "@smoke"] }, async ({ page }, info) => {
    test.skip(!process.env.NEWS_VISUAL, "screenshots compare only inside the Playwright Docker image");
    test.skip(!hasBaselines(info.project.name), "no baselines on this screen for this engine");
    await gotoPage(page, `/fixtures/components/${p}/`);
    const islands = page.locator("[data-island]");
    for (let i = 0; i < (await islands.count()); i++) {
      const island = islands.nth(i);
      if (!(await island.isVisible())) continue; // hidden by CSS at this width: never hydrates
      await island.scrollIntoViewIfNeeded();
      await expect(island.locator("[data-hydrated='true']")).toHaveCount(1);
    }
    await page.evaluate(() => window.scrollTo(0, 0));
    // CSS pixels keep the committed PNGs small; the device scale still decides how lines are drawn.
    await expect(page.locator("main")).toHaveScreenshot(`${p.replace("/", "-")}.png`, { scale: "css" });
  });
}
