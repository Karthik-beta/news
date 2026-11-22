// Builder's browser checks for the #11 controls and states. Every gallery passes the shared checks in both themes
// (axe, reflow, 12px floor, 24px targets under the smoke projects' touch emulation, design fonts); form controls
// are 16px or larger; the loading state keeps old results at 50% opacity with no spinner; the keyboard goes
// through the report form in order and the error summary's links land on their fields. The tester extends this.
import {
  expect,
  expectAccessible,
  expectDesignFonts,
  expectNoSidewaysScroll,
  expectNoSmallText,
  expectTargetSize,
  gotoPage,
  runningAnimations,
  setTheme,
  test,
} from "../fixtures";
import { fixtureCase, galleryPath } from "./primitives";

const PARTS = [
  "Button",
  "TextField",
  "TextArea",
  "Select",
  "Checkbox",
  "SearchField",
  "StateMessage",
  "NotFound",
  "LoadingRegion",
  "ReportForm",
];

for (const name of PARTS) {
  test(`${name} gallery: accessible in both themes, reads at this screen size, in the design fonts`, {
    tag: ["@smoke", "@a11y"],
  }, async ({ page, theme }) => {
    await gotoPage(page, galleryPath(name));
    await expectAccessible(page);
    await expectNoSidewaysScroll(page);
    await expectNoSmallText(page);
    await expectTargetSize(page);
    await expectDesignFonts(page);
    await setTheme(page, theme === "light" ? "dark" : "light");
    await expectAccessible(page);
  });
}

test("form controls are at least 16px, so phones do not zoom in", { tag: "@smoke" }, async ({ page }) => {
  for (const name of ["TextField", "TextArea", "Select", "SearchField", "ReportForm"]) {
    await gotoPage(page, galleryPath(name));
    const sizes = await page
      .locator("main")
      .locator("input:not([type=hidden]):not([type=checkbox]), select, textarea")
      .evaluateAll((els) => els.map((el) => Number.parseFloat(getComputedStyle(el).fontSize)));
    expect(sizes.length, name).toBeGreaterThan(0);
    for (const s of sizes) expect(s, name).toBeGreaterThanOrEqual(16);
  }
});

test("required, optional and error states render", { tag: "@smoke" }, async ({ page }) => {
  await gotoPage(page, galleryPath("TextField"));
  await expect(page.getByRole("textbox", { name: "Page address (required)" })).toHaveAttribute("required", "");
  await expect(
    page.getByRole("textbox", { name: "Your email, if you'd like a reply (optional)" }).first(),
  ).toBeVisible();
  const error = fixtureCase(page, "error");
  const input = error.getByRole("textbox");
  await expect(input).toHaveAttribute("aria-invalid", "true");
  const [border, message] = await Promise.all([
    input.evaluate((el) => getComputedStyle(el).borderTopColor),
    error.locator(".field__error").evaluate((el) => getComputedStyle(el).color),
  ]);
  expect(border).toBe(message); // both in the error colour
  await expect(error.locator(".field__error svg")).toBeVisible();
});

test("the checkbox is a 24px target with its label beside it", { tag: "@smoke" }, async ({ page }) => {
  await gotoPage(page, galleryPath("Checkbox"));
  const box = await page.getByRole("checkbox", { name: "Primary sources only" }).first().boundingBox();
  expect(box?.width).toBeGreaterThanOrEqual(24);
  expect(box?.height).toBeGreaterThanOrEqual(24);
  await page.getByText("Primary sources only", { exact: true }).first().click();
  await expect(page.getByRole("checkbox", { name: "Primary sources only" }).first()).toBeChecked();
});

test("loading keeps the previous results at 50% opacity and shows no spinner", { tag: "@smoke" }, async ({ page }) => {
  await gotoPage(page, galleryPath("LoadingRegion"));
  const opacity = (c: string) =>
    fixtureCase(page, c)
      .locator(".loading-region__content")
      .evaluate((el) => getComputedStyle(el).opacity);
  expect(await opacity("results shown")).toBe("1");
  expect(await opacity("loading new results")).toBe("0.5");
  await expect(fixtureCase(page, "loading new results").getByRole("link")).toHaveCount(3);
  await expect(fixtureCase(page, "loading new results").getByRole("status")).toHaveText("Loading results");
  // No spinner at any point: nothing animates now, nor 300ms later, and there is no progress indicator.
  expect(await runningAnimations(page)).toEqual([]);
  await page.waitForTimeout(300);
  expect(await runningAnimations(page)).toEqual([]);
  await expect(page.locator("[role=progressbar], progress")).toHaveCount(0);
});

test("the keyboard goes through the report form in order; summary links land on their fields", {
  tag: ["@smoke", "@interaction"],
}, async ({ page, browserName }) => {
  // Playwright's WebKit on Windows never moves to links with Tab (Safari's default; see SourceRef.spec.ts).
  test.skip(browserName === "webkit", "WebKit does not tab to links by default");
  await gotoPage(page, galleryPath("ReportForm"));
  const form = fixtureCase(page, "with errors after sending");
  const links = form.locator(".error-summary").getByRole("link");
  await expect(links).toHaveText(["Say what is wrong.", /^Enter an email address/]);
  await links.first().focus();
  const order: string[] = [];
  for (let i = 0; i < 6; i++) {
    order.push(
      await page.evaluate(() => {
        const el = document.activeElement as HTMLElement | null;
        return el?.id || el?.textContent?.trim() || "";
      }),
    );
    await page.keyboard.press("Tab");
  }
  expect(order).toEqual([
    "Say what is wrong.",
    order[1] ?? "",
    "report-what",
    "report-where",
    "report-email",
    "Send report",
  ]);
  expect(order[1]).toMatch(/^Enter an email address/);
  // Following a summary link brings its field into view.
  await links.nth(1).focus();
  await page.keyboard.press("Enter");
  await expect(page).toHaveURL(/#report-email$/);
  await expect(page.locator("#report-email")).toBeInViewport();
});
