// The site shell (src/pages/index.astro) on every screen. Also the reference for how page, fixture-page and
// component specs use the shared checks in ./fixtures.ts. Tags: see site/TESTING.md.

import { tokenValue } from "../support/tokens";
import {
  expect,
  expectAccessible,
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
