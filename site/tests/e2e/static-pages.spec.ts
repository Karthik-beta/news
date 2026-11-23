// The reader-facing static pages on every screen and in both themes (tags: see site/TESTING.md). Run in CI as the
// smoke projects: Chromium at 360 and 1440, light and dark.
import {
  expect,
  expectAccessible,
  expectDesignFonts,
  expectNoSidewaysScroll,
  expectNoSmallText,
  expectTargetSize,
  gotoPage,
  test,
} from "./fixtures";

const PAGES = [
  "/methodology/",
  "/about/",
  "/glossary/",
  "/privacy/",
  "/legal/",
  "/contact/",
  "/report/",
  "/data/",
  "/corrections/",
  "/feeds/",
];

for (const path of PAGES) {
  test.describe(`static page ${path}`, () => {
    test.beforeEach(async ({ page }) => {
      await gotoPage(page, path);
    });

    test("reads at this screen size", { tag: "@smoke" }, async ({ page }) => {
      await expect(page.getByRole("heading", { level: 1 })).toHaveCount(1);
      await expect(page.getByRole("heading", { level: 1 })).toBeVisible();
      await expectNoSidewaysScroll(page);
      await expectNoSmallText(page);
      await expectTargetSize(page);
      await expectDesignFonts(page);
    });

    test("has no WCAG 2.2 AA violations", { tag: ["@smoke", "@a11y"] }, async ({ page }) => {
      await expectAccessible(page);
    });
  });
}

test.describe("static page interactions", () => {
  test("a glossary term opens its definition, and the page stays accessible", { tag: ["@smoke", "@a11y"] }, async ({
    page,
  }) => {
    await gotoPage(page, "/methodology/");
    await page.locator("button.glossary-term").first().click();
    await expect(page.getByRole("dialog")).toBeVisible();
    await expectAccessible(page);
    await page.keyboard.press("Escape");
    await expect(page.getByRole("dialog")).toBeHidden();
  });

  test("the glossary jump row moves to the letter", { tag: "@smoke" }, async ({ page }) => {
    await gotoPage(page, "/glossary/");
    await page.getByRole("link", { name: "M", exact: true }).click();
    await expect(page).toHaveURL(/#letter-m$/);
    await expect(page.locator("#letter-m")).toBeInViewport();
  });
});
