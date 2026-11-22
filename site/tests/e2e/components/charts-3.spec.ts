// Charts III part 1 (#13): SmallMultiples, Heatmap and Histogram in the browser, on their fixture pages
// (/fixtures/components/<Name>/<fixture>/, built with NEWS_FIXTURES=1). Builder's smoke checks; the tester's full
// procedure and sign-off are news-component-testing. axe finds nothing in light or dark; each page reads at this
// screen size; the plots answer the keys; small multiples' panels are the same size.
import {
  expect,
  expectAccessible,
  expectDesignFonts,
  expectNoSidewaysScroll,
  expectNoSmallText,
  expectTargetSize,
  gotoPage,
  setTheme,
  test,
} from "../fixtures";

const PAGES = ["SmallMultiples", "Heatmap", "Histogram"].flatMap((n) => [`${n}/default`, `${n}/edge-cases`]);
const url = (p: string) => `/fixtures/components/${p}/`;

async function hydrateVisible(page: import("@playwright/test").Page): Promise<void> {
  const islands = page.locator("[data-island]");
  for (let i = 0; i < (await islands.count()); i++) {
    const island = islands.nth(i);
    if (!(await island.isVisible())) continue;
    await island.scrollIntoViewIfNeeded();
    await expect(island.locator("[data-hydrated='true']")).toHaveCount(1);
  }
}

for (const p of PAGES) {
  test(`${p}: no WCAG 2.2 AA violations in light and dark`, { tag: ["@smoke", "@a11y"] }, async ({ page, theme }) => {
    await gotoPage(page, url(p));
    await hydrateVisible(page);
    await expectAccessible(page);
    await setTheme(page, theme === "light" ? "dark" : "light");
    await expectAccessible(page);
  });

  test(`${p}: reads at this screen size`, { tag: "@smoke" }, async ({ page }) => {
    await gotoPage(page, url(p));
    await hydrateVisible(page);
    await expectNoSidewaysScroll(page);
    await expectNoSmallText(page);
    await expectTargetSize(page);
    await expectDesignFonts(page);
  });
}

test("the plots are one tab stop each, and the keys move the tooltip", { tag: ["@smoke", "@a11y"] }, async ({
  page,
}) => {
  for (const p of ["SmallMultiples/default", "Heatmap/default", "Histogram/default"]) {
    await gotoPage(page, url(p));
    await hydrateVisible(page);
    const plot = page.locator("[data-island] [role='img'][tabindex='0']").first();
    await plot.focus();
    await page.keyboard.press("ArrowRight");
    await expect(page.locator(".chart-tooltip").first(), p).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(page.locator(".chart-tooltip"), p).toHaveCount(0);
  }
});

test("a heatmap's up and down keys move a row", { tag: ["@smoke", "@a11y"] }, async ({ page }) => {
  await gotoPage(page, url("Heatmap/default"));
  await hydrateVisible(page);
  const grid = page.locator("figure", { has: page.locator("#margins-title") }).locator("[role='img'][tabindex='0']");
  await grid.focus();
  await page.keyboard.press("ArrowRight");
  await expect(page.locator(".chart-tooltip")).toContainText("Services");
  await page.keyboard.press("ArrowDown");
  await expect(page.locator(".chart-tooltip")).toContainText("Products");
});

test("small multiples' panels are the same size, on one scale", { tag: "@smoke" }, async ({ page }) => {
  await gotoPage(page, url("SmallMultiples/edge-cases"));
  const panels = page.locator("figure", { has: page.locator("#many-title") }).locator(".chart-plot__inner");
  const boxes = await panels.evaluateAll((els) => els.map((e) => e.getBoundingClientRect().toJSON()));
  expect(boxes.length).toBe(7); // the eighth series has no values: its panel says No data and draws no plot
  for (const b of boxes) {
    expect(Math.round(b.width)).toBe(Math.round(boxes[0].width));
    expect(Math.round(b.height)).toBe(Math.round(boxes[0].height));
  }
  // Two across on phones.
  if ((page.viewportSize()?.width ?? 0) < 600) expect(new Set(boxes.map((b) => Math.round(b.x))).size).toBe(2);
});
