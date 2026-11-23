// ChoroplethMap (#13) in the browser, on its fixture pages. Builder's smoke checks; the tester's full procedure and
// sign-off are news-component-testing. axe finds nothing in light or dark; the page reads at this screen size; at
// phone width the map falls back to ranked bars under a small map, and wider it shows the full map and the table.
import {
  expect,
  expectAccessible,
  expectDesignFonts,
  expectNoSidewaysScroll,
  expectNoSmallText,
  gotoPage,
  setTheme,
  test,
} from "../fixtures";

const PAGES = ["ChoroplethMap/default", "ChoroplethMap/edge-cases"];
const url = (p: string) => `/fixtures/components/${p}/`;

for (const p of PAGES) {
  test(`${p}: no WCAG 2.2 AA violations in light and dark`, { tag: ["@smoke", "@a11y"] }, async ({ page, theme }) => {
    await gotoPage(page, url(p));
    await expectAccessible(page);
    await setTheme(page, theme === "light" ? "dark" : "light");
    await expectAccessible(page);
  });

  test(`${p}: reads at this screen size`, { tag: "@smoke" }, async ({ page }) => {
    await gotoPage(page, url(p));
    await expectNoSidewaysScroll(page);
    await expectNoSmallText(page);
    await expectDesignFonts(page);
  });
}

test("phones get ranked bars under a small map; wider screens the full map and the ranked table", {
  tag: "@smoke",
}, async ({ page }) => {
  await gotoPage(page, url("ChoroplethMap/default"));
  const fig = page.locator("figure", { has: page.locator("#g7-title") });
  const bars = fig.locator(".choropleth__bars");
  const table = fig.locator(".choropleth__table");
  const map = fig.locator("svg.choropleth__map");
  const plotWidth = await fig.evaluate((f) => f.getBoundingClientRect().width);
  if ((page.viewportSize()?.width ?? 0) <= 360) {
    await expect(bars).toBeVisible();
    await expect(table).toBeHidden();
    await expect(bars.locator(".bar-chart__row")).toHaveCount(7);
    const mapWidth = await map.evaluate((m) => m.getBoundingClientRect().width);
    expect(mapWidth).toBeLessThanOrEqual(plotWidth);
  } else {
    await expect(table).toBeVisible();
    await expect(bars).toBeHidden();
    await expect(table.locator("tbody tr")).toHaveCount(7);
    expect(await map.evaluate((m) => m.getBoundingClientRect().width)).toBeCloseTo(plotWidth, -1);
  }
  // Hatched regions draw their pattern (the pattern is in the same SVG).
  const fill = await fig.locator("path.is-missing").first().getAttribute("fill");
  await expect(fig.locator(`pattern${fill?.slice(4, -1)}`)).toHaveCount(1);
});
