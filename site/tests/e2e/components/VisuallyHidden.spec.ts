// VisuallyHidden in browsers, on every screen (news-component-testing). Gallery: /fixtures/primitives/VisuallyHidden/.
import { expect, gotoPage, test } from "../fixtures";
import { fixtureCase, galleryChecks, galleryPath } from "./primitives";

const PATH = galleryPath("VisuallyHidden");

galleryChecks("VisuallyHidden", 2);

const box = (el: Element) => {
  const r = el.getBoundingClientRect();
  return { w: r.width, h: r.height, clip: getComputedStyle(el).clipPath };
};

test.describe("VisuallyHidden", () => {
  test("hides the text on screen and keeps it for assistive technology", async ({ page }) => {
    await gotoPage(page, PATH);
    const c = fixtureCase(page, "hidden text completes a sentence");
    const hidden = c.locator(".visually-hidden");
    const b = await hidden.evaluate(box);
    expect(b.w).toBeLessThanOrEqual(1);
    expect(b.h).toBeLessThanOrEqual(1);
    expect(b.clip).toMatch(/inset\(50%\)/);
    // Not on screen, but in the accessibility tree as part of the sentence.
    await expect(c.locator("p")).toMatchAriaSnapshot("- paragraph: Net sales in fiscal 2024 were $391.0bn");
    // The visible line keeps its height (nothing collapses or jumps).
    const p = await c.locator("p").boundingBox();
    expect(p?.height).toBeGreaterThan(10);
  });

  test("a focusable skip link appears while it has focus, with a focus ring, and hides again", {
    tag: "@interaction",
  }, async ({ page, browserName }) => {
    await gotoPage(page, PATH);
    const wrapper = fixtureCase(page, "focusable (skip link)").locator(".visually-hidden--focusable");
    const link = wrapper.getByRole("link", { name: "Skip to content" });
    expect((await wrapper.evaluate(box)).w).toBeLessThanOrEqual(1);
    // Reach it by keyboard, the way readers do (WebKit tabs to links only with Option, so focus it directly there).
    if (browserName === "webkit") await link.focus();
    else {
      await page.locator("body").focus();
      for (let i = 0; i < 5 && !(await link.evaluate((el) => el === document.activeElement)); i++) {
        await page.keyboard.press("Tab");
      }
    }
    await expect(link).toBeFocused();
    const shown = await link.evaluate((el) => {
      const r = el.getBoundingClientRect();
      const s = getComputedStyle(el);
      return { w: r.width, h: r.height, outline: [s.outlineStyle, s.outlineWidth] };
    });
    expect(shown.w).toBeGreaterThan(24);
    expect(shown.h).toBeGreaterThan(12);
    expect(shown.outline).toEqual(["solid", "2px"]);
    await link.blur();
    expect((await wrapper.evaluate(box)).w).toBeLessThanOrEqual(1);
  });

  test("hidden text stays hidden in print", { tag: "@print" }, async ({ page }) => {
    await gotoPage(page, PATH);
    await page.emulateMedia({ media: "print" });
    const b = await fixtureCase(page, "hidden text completes a sentence").locator(".visually-hidden").evaluate(box);
    expect(b.w).toBeLessThanOrEqual(1);
  });
});
