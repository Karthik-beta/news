// Charts II (#13, part 1) in the browser, on their fixture pages (/fixtures/components/<Name>/<fixture>/, built with
// NEWS_FIXTURES=1). Builder's smoke checks; the tester's full procedure and sign-off are news-component-testing.
// axe finds nothing in light or dark; each page reads at this screen size; the Sankey shows its waterfall on phones
// and its flows on wide screens; the plots answer the keys.
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

const PAGES = ["Waterfall", "Sankey", "StackedBar", "SegmentedBar", "DotPlot", "Slope"].flatMap((n) => [
  `${n}/default`,
  `${n}/edge-cases`,
]);
const url = (p: string) => `/fixtures/components/${p}/`;

/** Brings every visible island into view and waits until it has hydrated. Islands hidden by CSS (the Sankey's
 * flows on a phone, its waterfall on a wide screen) are never seen, so they never hydrate. */
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

test(
  "the Sankey shows its waterfall on phones and its flows on wide screens",
  { tag: "@smoke" },
  async ({ page }, info) => {
    await gotoPage(page, url("Sankey/default"));
    const figure = page.locator("figure", { has: page.locator("#flows-title") });
    const flows = figure.locator(".sankey__flows");
    const fallback = figure.locator(".sankey__fallback");
    const width = page.viewportSize()?.width ?? 0;
    if (width < 600) {
      await expect(fallback).toBeVisible();
      await expect(flows).toBeHidden();
      await expect(fallback.locator(".bar-chart__row")).toHaveCount(9);
      await expect(fallback.locator(".bar-chart__value", { hasText: "−$210.4bn" })).toBeVisible();
    } else if (width >= 980) {
      await expect(flows).toBeVisible();
      await expect(fallback).toBeHidden();
      await expect(flows.locator(".sankey__node")).toHaveCount(14);
      await expect(flows.locator(".sankey__value", { hasText: "$93.7bn" })).toBeVisible();
      // The flows' plot answers the keys too: one tab stop, the arrows step through the nodes.
      await hydrateVisible(page);
      await flows.locator("[role='img'][tabindex='0']").focus();
      await page.keyboard.press("ArrowRight");
      await expect(flows.locator(".chart-tooltip")).toContainText("iPhone");
    } else {
      info.skip(true, "between the phone and desktop widths the choice depends on the plot's width in em");
    }
  },
);

test("a Sankey that cannot flow shows its waterfall at every width", { tag: "@smoke" }, async ({ page }) => {
  await gotoPage(page, url("Sankey/edge-cases"));
  for (const id of ["loss", "mismatch"]) {
    const figure = page.locator("figure", { has: page.locator(`#${id}-title`) });
    await expect(figure.locator(".sankey__fallback")).toBeVisible();
    await expect(figure.locator(".sankey__flows")).toHaveCount(0);
  }
});

test("the plots are one tab stop each, and the arrow keys move the tooltip", { tag: ["@smoke", "@a11y"] }, async ({
  page,
}) => {
  for (const p of [
    "Waterfall/default",
    "StackedBar/default",
    "SegmentedBar/default",
    "DotPlot/default",
    "Slope/default",
  ]) {
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
