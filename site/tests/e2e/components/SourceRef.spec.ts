// SourceRef and its citation popover in browsers, on every screen (news-component-testing). Gallery:
// /fixtures/primitives/SourceRef/; in context: the prose and key figures of the renderer fixture pages.
// Spec: design/components.md ("Key figures", "Citation popover"); data: the edge-cases and Apple bundles.
import { expect, expectAccessible, gotoPage, setTheme, test } from "../fixtures";
import {
  expectPopoverLook,
  expectPopoverPlacement,
  fixtureCase,
  galleryChecks,
  galleryPath,
  panelFor,
  tokenRgb,
} from "./primitives";

const PATH = galleryPath("SourceRef");

galleryChecks("SourceRef", 13);

/** What each case shows on the page, from the fixture data (formatted by code). */
const SHOWN: Record<string, string> = {
  "money, compact": "$391.0bn",
  "money, as reported": "$391,035 million",
  "derived value (calculated by News)": "2.0%",
  percentage: "2.4%",
  zero: "0%",
  negative: "−5.7%",
  "very large": "$123,456.8trn",
  "very small": "0.000000012",
  rupees: "₹2.1trn",
};

test.describe("SourceRef", () => {
  test("shows every value formatted by code, and missing data as 'No data', never a zero", async ({ page }) => {
    await gotoPage(page, PATH);
    for (const [name, text] of Object.entries(SHOWN)) {
      await expect(fixtureCase(page, name).locator("button.source-ref"), name).toHaveText(text);
    }
    for (const name of ["no value published (null)", "unknown reference"]) {
      const c = fixtureCase(page, name);
      await expect(c.locator(".source-ref--missing")).toHaveText("No data");
      await expect(c.locator("button")).toHaveCount(0);
    }
  });

  test("a cited figure reads as text with a dotted ink-3 underline, not as a button", async ({ page, theme }) => {
    await gotoPage(page, PATH);
    const trigger = fixtureCase(page, "money, compact").locator("button.source-ref");
    const s = await trigger.evaluate((el) => {
      const c = getComputedStyle(el);
      const p = getComputedStyle(el.parentElement as HTMLElement);
      return {
        font: [c.fontFamily === p.fontFamily, c.fontSize === p.fontSize, c.lineHeight === p.lineHeight],
        color: [c.color, p.color],
        decoration: [c.textDecorationLine, c.textDecorationStyle, c.textDecorationColor],
        box: [c.backgroundColor, c.borderTopWidth, c.paddingLeft],
      };
    });
    expect(s.font, "same family, size and line height as the sentence").toEqual([true, true, true]);
    expect(s.color[0]).toBe(s.color[1]);
    expect(s.decoration).toEqual(["underline", "dotted", tokenRgb("--color-ink-3", theme)]);
    expect(s.box).toEqual(["rgba(0, 0, 0, 0)", "0px", "0px"]);
    // Missing data is said in secondary ink.
    const missing = fixtureCase(page, "unknown reference").locator(".source-ref--missing");
    expect(await missing.evaluate((el) => getComputedStyle(el).color)).toBe(tokenRgb("--color-ink-2", theme));
  });

  test("a click opens the citation popover with the exact value, document, location, date and link", {
    tag: "@interaction",
  }, async ({ page, theme, browserName }) => {
    await gotoPage(page, PATH);
    const trigger = fixtureCase(page, "money, compact").locator("button.source-ref");
    const panel = await panelFor(page, trigger);
    await expect(panel).toBeHidden();
    await trigger.click();
    await expect(panel).toBeVisible();
    await expect(panel).toHaveAccessibleName("Source for $391.0bn");
    await expect(panel.locator(".popover__value")).toHaveText("$391,035 million");
    await expect(panel).toContainText("Apple Inc. Form 10-K, fiscal 2024");
    await expect(panel).toContainText("XBRL tag us-gaap:");
    await expect(panel).toContainText(/Retrieved \d{1,2} [A-Z][a-z]+ \d{4}/);
    await expect(panel.getByRole("link", { name: "Open the source" })).toHaveAttribute("href", /^https:\/\//);
    await expectPopoverPlacement(page, trigger, panel);
    await expectPopoverLook(panel, theme);
    await setTheme(page, theme === "light" ? "dark" : "light");
    if (browserName === "firefox") {
      // Firefox 159 keeps the old colours of a popover placed by a position-try fallback (flipped above, moved in
      // from the edges, capped) until it is laid out again: an engine bug, reproduced without our CSS. Readers
      // switch theme with a button, which closes an open popover, so re-open it before checking the colours.
      test.info().annotations.push({ type: "firefox", description: "popover re-opened after the theme switch" });
      await page.keyboard.press("Escape");
      await trigger.click();
    }
    await expectPopoverLook(panel, theme === "light" ? "dark" : "light");
    await expectAccessible(page);
  });

  test("keyboard: Enter and Space open it, Tab reaches the source link, Escape closes and returns focus", {
    tag: "@interaction",
  }, async ({ page, browserName }) => {
    await gotoPage(page, PATH);
    const trigger = fixtureCase(page, "money, compact").locator("button.source-ref");
    const panel = await panelFor(page, trigger);
    await trigger.focus();
    const ring = await trigger.evaluate((el) => [getComputedStyle(el).outlineStyle, getComputedStyle(el).outlineWidth]);
    expect(ring, "visible focus ring").toEqual(["solid", "2px"]);
    await page.keyboard.press("Enter");
    await expect(panel).toBeVisible();
    const link = panel.getByRole("link", { name: "Open the source" });
    if (browserName === "webkit") {
      // Playwright's WebKit on Windows never tabs to links (Safari does only with "Press Tab to highlight each item"
      // or Option-Tab), so check the link is focusable inside the open panel instead.
      test.info().annotations.push({ type: "webkit", description: "Tab to links not emulated; link focused directly" });
      await link.focus();
    } else {
      await page.keyboard.press("Tab");
    }
    await expect(link).toBeFocused();
    await page.keyboard.press("Escape");
    await expect(panel).toBeHidden();
    await expect(trigger).toBeFocused();
    await page.keyboard.press(" ");
    await expect(panel).toBeVisible();
    await page.keyboard.press(" ");
    await expect(panel).toBeHidden();
  });

  test("a click outside closes it, and opening another closes the first", { tag: "@interaction" }, async ({ page }) => {
    await gotoPage(page, PATH);
    const first = fixtureCase(page, "money, compact").locator("button.source-ref");
    const second = fixtureCase(page, "money, as reported").locator("button.source-ref");
    const firstPanel = await panelFor(page, first);
    await first.click();
    await expect(firstPanel).toBeVisible();
    await page.mouse.click(2, 2);
    await expect(firstPanel).toBeHidden();
    await first.click();
    // The open panel may cover the next figure (it sits in the top layer), so open the second from the keyboard.
    await second.focus();
    await page.keyboard.press("Enter");
    await expect(firstPanel).toBeHidden();
    await expect(await panelFor(page, second)).toBeVisible();
  });

  test("a tap opens it and a tap outside closes it", { tag: "@interaction" }, async ({ page, hasTouch }) => {
    test.skip(!hasTouch, "touch screens only");
    await gotoPage(page, PATH);
    const trigger = fixtureCase(page, "money, compact").locator("button.source-ref");
    const panel = await panelFor(page, trigger);
    await trigger.tap();
    await expect(panel).toBeVisible();
    await page.touchscreen.tap(2, 2);
    await expect(panel).toBeHidden();
  });

  test("every popover on the page opens on screen, next to its figure, without moving the page", {
    tag: "@interaction",
  }, async ({ page }) => {
    await gotoPage(page, PATH);
    const triggers = await page.locator("button.source-ref").all();
    expect(triggers.length).toBe(11);
    for (const trigger of triggers) {
      await trigger.scrollIntoViewIfNeeded();
      const before = await trigger.boundingBox();
      await trigger.click();
      const panel = await panelFor(page, trigger);
      await expect(panel).toBeVisible();
      await expectPopoverPlacement(page, trigger, panel);
      expect(await trigger.boundingBox(), "opening does not move the text").toEqual(before);
      await page.keyboard.press("Escape");
    }
  });

  test("a capped popover (landscape phone) scrolls inside, and its source link comes into view on screen", {
    tag: "@interaction",
  }, async ({ page }) => {
    await gotoPage(page, PATH);
    let capped = 0;
    for (const trigger of await page.locator("button.source-ref").all()) {
      await trigger.scrollIntoViewIfNeeded();
      await trigger.click();
      const panel = await panelFor(page, trigger);
      await expect(panel).toBeVisible();
      const scrolls = await panel.evaluate((el) => el.scrollHeight > el.clientHeight + 1);
      if (scrolls) capped += 1;
      const link = panel.getByRole("link", { name: "Open the source" }).last();
      await link.focus();
      await expect(link).toBeFocused();
      const [lb, pb] = [await link.boundingBox(), await panel.boundingBox()];
      const vh = page.viewportSize()?.height ?? 0;
      expect(lb && pb, "boxes").toBeTruthy();
      if (lb && pb) {
        expect(lb.y, "link inside the panel (top)").toBeGreaterThanOrEqual(pb.y - 0.5);
        expect(lb.y + lb.height, "link inside the panel (bottom)").toBeLessThanOrEqual(pb.y + pb.height + 0.5);
        expect(lb.y + lb.height, "link on screen").toBeLessThanOrEqual(vh + 0.5);
      }
      await page.keyboard.press("Escape");
    }
    test.info().annotations.push({ type: "capped popovers", description: String(capped) });
  });

  test("stand-alone figures (key-figure size) are at least 24 x 24px to tap", async ({ page }) => {
    await gotoPage(page, PATH);
    for (const name of ["percentage", "zero", "negative", "rupees"]) {
      const box = await fixtureCase(page, name).locator("button.source-ref").boundingBox();
      expect(box?.width, name).toBeGreaterThanOrEqual(24);
      expect(box?.height, name).toBeGreaterThanOrEqual(24);
    }
  });

  test("hover does not open the popover (open design question: components.md asks for hover and focus)", {
    tag: "@interaction",
  }, async ({ page, isMobile }) => {
    test.skip(isMobile, "no hover on touch screens");
    await gotoPage(page, PATH);
    const trigger = fixtureCase(page, "money, compact").locator("button.source-ref");
    await trigger.hover();
    await page.waitForTimeout(300);
    const opened = await (await panelFor(page, trigger)).isVisible();
    test.info().annotations.push({
      type: "known design question",
      description: `hover opens the popover: ${opened}; focus alone does not (click, tap, Enter, Space only)`,
    });
    expect(opened).toBe(false);
  });

  test("prints every cited figure as text and no popover", { tag: "@print" }, async ({ page }) => {
    await gotoPage(page, PATH);
    await page.emulateMedia({ media: "print" });
    // innerText is what is rendered (and printed); textContent would include the hidden popover.
    const sentence = fixtureCase(page, "money, compact").locator("p");
    // biome-ignore lint/performance/useDomNodeTextContent: the rendered text is the point (hidden parts excluded)
    expect(await sentence.evaluate((el) => (el as HTMLElement).innerText.trim())).toBe(
      "Net sales were $391.0bn in fiscal 2024.",
    );
    for (const [name, text] of Object.entries(SHOWN)) {
      // The popover holds the same text for an as-reported figure; it is in the DOM but not printed.
      await expect(
        fixtureCase(page, name).getByText(text, { exact: true }).filter({ visible: true }),
        name,
      ).toHaveCount(1);
    }
    await expect(page.locator("[popover]").first()).toBeHidden();
  });

  test("in context: popovers in the article prose and key figures open on screen", { tag: "@interaction" }, async ({
    page,
  }) => {
    for (const path of ["/fixtures/renderer/apple-fy2024/", "/fixtures/renderer/static/"]) {
      await gotoPage(page, path);
      const triggers = await page.locator("main button.source-ref").all();
      expect(triggers.length, path).toBeGreaterThan(0);
      for (const trigger of triggers) {
        await trigger.scrollIntoViewIfNeeded();
        await trigger.click();
        const panel = await panelFor(page, trigger);
        await expect(panel).toBeVisible();
        await expectPopoverPlacement(page, trigger, panel);
        await page.keyboard.press("Escape");
      }
    }
  });

  test("in context: prints the article's cited figures", { tag: "@print" }, async ({ page }) => {
    await gotoPage(page, "/fixtures/renderer/static/");
    await page.emulateMedia({ media: "print" });
    // biome-ignore lint/performance/useDomNodeTextContent: the rendered text is the point (hidden parts excluded)
    const printed = await page.locator("main").evaluate((el) => (el as HTMLElement).innerText.replace(/\s+/g, " "));
    expect(printed).toContain("$391.0bn");
    expect(printed).not.toContain("$391,035 million"); // the popover itself does not print
  });
});
