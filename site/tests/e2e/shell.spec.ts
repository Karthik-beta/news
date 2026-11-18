// The site shell (src/pages/index.astro) on every screen. Also the reference for how page, fixture-page and
// component specs use the shared checks in ./fixtures.ts. Tags: see site/TESTING.md.

import { tokenValue } from "../support/tokens";
import {
  cumulativeLayoutShift,
  expect,
  expectAccessible,
  expectDesignFonts,
  expectNoSidewaysScroll,
  expectNoSmallText,
  expectTargetSize,
  expectTheme,
  gotoPage,
  longestMotionMs,
  runningAnimations,
  setTextSize,
  setTheme,
  test,
  watchLayoutShift,
  zoomTo200,
} from "./fixtures";

const opposite = (t: "light" | "dark") => (t === "light" ? "dark" : "light");

test.describe("site shell", () => {
  test.beforeEach(async ({ page }) => {
    await gotoPage(page, "/");
  });

  test("reads at this screen size", { tag: "@smoke" }, async ({ page }) => {
    await expect(page.getByRole("heading", { level: 1 })).toBeVisible();
    await expect(page.getByRole("heading", { level: 1 })).toHaveCount(1);
    await expectNoSidewaysScroll(page);
    await expectNoSmallText(page);
    await expectTargetSize(page);
  });

  test("sets all text in the two design typefaces", { tag: "@smoke" }, async ({ page }) => {
    await expectDesignFonts(page);
  });

  test("does not shift layout after load (CLS 0)", async ({ page, browserName }) => {
    // Shifts once the fonts are in must be zero. The CLS since navigation also counts the font swap, which
    // depends on whether the self-hosted fonts beat first paint; it is recorded, not asserted (TESTING.md).
    const shiftSinceFonts = await watchLayoutShift(page);
    const total = await cumulativeLayoutShift(page);
    test.skip(total === null, `${browserName} does not report layout shifts`);
    test.info().annotations.push({ type: "CLS since navigation, font swap included", description: String(total) });
    expect(await shiftSinceFonts()).toBe(0);
  });

  test("uses the gutter for this breakpoint (design/layout.md)", { tag: "@smoke" }, async ({ page }) => {
    const width = page.viewportSize()?.width ?? 0;
    const expected =
      width >= 980 ? "--layout-gutter-desktop" : width >= 600 ? "--layout-gutter-tablet" : "--layout-gutter-phone";
    const padding = await page.locator("main").evaluate((el) => getComputedStyle(el).paddingLeft);
    expect(padding).toBe(tokenValue(expected));
  });

  test("has no WCAG 2.2 AA violations", { tag: ["@smoke", "@a11y"] }, async ({ page }) => {
    await expectAccessible(page);
  });

  test("follows the system colour scheme", { tag: "@smoke" }, async ({ page, theme }) => {
    await expectTheme(page, theme);
  });

  test("lets data-theme override the system setting, both ways", { tag: "@smoke" }, async ({ page, theme }) => {
    await setTheme(page, opposite(theme));
    await expectTheme(page, opposite(theme));
    await expectAccessible(page);
    await setTheme(page, theme);
    await expectTheme(page, theme);
    await setTheme(page, "system");
    await expectTheme(page, theme);
  });

  test("keeps all content at 200% text size, without sideways scroll", { tag: "@zoom" }, async ({ page }) => {
    // biome-ignore lint/performance/useDomNodeTextContent: rendered text (innerText) is what must not be lost
    const before = await page.locator("main").innerText();
    await setTextSize(page, 200);
    await expect(page.getByRole("heading", { level: 1 })).toBeVisible();
    await expect(page.locator("main")).toHaveText(before, { useInnerText: true });
    await expectNoSidewaysScroll(page);
  });

  test("reflows at 200% browser zoom, without sideways scroll", { tag: "@zoom" }, async ({ page }) => {
    // biome-ignore lint/performance/useDomNodeTextContent: rendered text (innerText) is what must not be lost
    const before = await page.locator("main").innerText();
    test.skip(!(await zoomTo200(page)), "below 640px wide, 200% zoom is the 320 project (400%)");
    await expect(page.locator("main")).toHaveText(before, { useInnerText: true });
    await expectNoSidewaysScroll(page);
    await expectNoSmallText(page);
  });

  test("has no motion under reduced motion", async ({ page }) => {
    expect(await runningAnimations(page)).toEqual([]);
    expect(await longestMotionMs(page)).toBe(0);
  });

  test("never animates on load, and transitions stay within 200ms", { tag: "@motion" }, async ({ page }) => {
    expect(await runningAnimations(page)).toEqual([]);
    expect(await longestMotionMs(page)).toBeLessThanOrEqual(200);
  });

  test("prints the article content", { tag: "@print" }, async ({ page }) => {
    await page.emulateMedia({ media: "print" });
    await expect(page.getByRole("heading", { level: 1 })).toBeVisible();
    await expect(page.locator("main")).toBeVisible();
  });

  test("keeps text and structure in forced colours", { tag: "@forced-colors" }, async ({ page }) => {
    expect(await page.evaluate(() => matchMedia("(forced-colors: active)").matches)).toBe(true);
    await expect(page.getByRole("heading", { level: 1 })).toBeVisible();
    await expectAccessible(page);
  });

  // Screenshot baseline example. Disabled until fonts are self-hosted (Phase 3.1) and the first fixture page
  // exists; until then the shell is a placeholder and system-font fallbacks make pixels meaningless.
  // Baselines are made and compared only on Linux in the Playwright image: `bun run test:visual`.
  test("matches its screenshot baseline", { tag: "@visual" }, async ({ page }) => {
    test.fixme(true, "enable with self-hosted fonts in Phase 3.1 (TESTING.md, Screenshots)");
    test.skip(!process.env.NEWS_VISUAL, "screenshots compare only inside the Playwright Docker image");
    await expect(page).toHaveScreenshot("shell.png", { fullPage: true });
  });
});

test.describe("site shell without JavaScript", () => {
  test.use({ javaScriptEnabled: false });

  test("renders the complete page as static HTML", { tag: "@smoke" }, async ({ page }) => {
    await page.goto("/");
    await expect(page.getByRole("heading", { level: 1 })).toBeVisible();
    await expect(page.locator("main p").first()).toBeVisible();
  });
});
