// The eight icons in browsers, on every screen (news-component-testing). Gallery: /fixtures/primitives/Icon/.
// Spec: design/components.md ("Icons"): 1.6px strokes on a 16px grid, in the text colour around them.
import { expect, gotoPage, setTheme, test } from "../fixtures";
import { fixtureCase, galleryChecks, galleryPath, tokenRgb } from "./primitives";

const PATH = galleryPath("Icon");
const NAMES = ["search", "external", "download", "close", "play", "error", "check", "info"];

galleryChecks("Icon", 10);

test.describe("Icon", () => {
  test("each icon is 1em square, follows the text colour and stays hidden from assistive technology", async ({
    page,
    theme,
  }) => {
    await gotoPage(page, PATH);
    for (const t of [theme, theme === "light" ? "dark" : "light"] as const) {
      await setTheme(page, t);
      for (const name of NAMES) {
        const s = await fixtureCase(page, name)
          .locator("svg.icon")
          .evaluate((svg) => {
            const r = svg.getBoundingClientRect();
            const cs = getComputedStyle(svg);
            const parent = getComputedStyle(svg.parentElement as HTMLElement);
            return {
              w: r.width,
              h: r.height,
              size: Number.parseFloat(parent.fontSize),
              stroke: cs.stroke,
              color: cs.color,
              strokeWidth: cs.strokeWidth,
              hidden: svg.getAttribute("aria-hidden"),
            };
          });
        expect(s.w, name).toBeCloseTo(s.size, 0);
        expect(s.h, name).toBeCloseTo(s.size, 0);
        expect(s.stroke, name).toBe(s.color);
        expect(s.color, name).toBe(tokenRgb("--color-ink", t));
        expect(s.strokeWidth, name).toBe("1.6px");
        expect(s.hidden).toBe("true");
      }
      // Inside a fly title the icon takes the crimson of the text around it.
      const fly = await fixtureCase(page, "inherits colour and size")
        .locator("svg")
        .evaluate((svg) => getComputedStyle(svg).stroke);
      expect(fly).toBe(tokenRgb("--color-brand", t));
    }
  });

  test("a labelled icon is an image with its name", async ({ page }) => {
    await gotoPage(page, PATH);
    await expect(fixtureCase(page, "labelled, standing alone").getByRole("img", { name: "Search" })).toBeVisible();
  });

  test("filled parts (play, error, info) fill with the text colour and draw no stroke", async ({ page }) => {
    await gotoPage(page, PATH);
    for (const name of ["play", "error", "info"]) {
      const s = await fixtureCase(page, name)
        .locator(".icon__fill")
        .first()
        .evaluate((el) => ({
          fill: getComputedStyle(el).fill,
          stroke: getComputedStyle(el).stroke,
          color: getComputedStyle(el).color,
        }));
      expect(s.fill, name).toBe(s.color);
      expect(s.stroke, name).toBe("none");
    }
  });

  test("follows the system text colour in forced colours", { tag: "@forced-colors" }, async ({ page }) => {
    await gotoPage(page, PATH);
    for (const name of NAMES) {
      const s = await fixtureCase(page, name)
        .locator("svg.icon")
        .evaluate((svg) => ({
          stroke: getComputedStyle(svg).stroke,
          bg: getComputedStyle(document.body).backgroundColor,
        }));
      expect(s.stroke, name).not.toBe(s.bg);
    }
  });
});
