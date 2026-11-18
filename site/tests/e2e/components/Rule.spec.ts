// Rule in browsers, on every screen (news-component-testing). Gallery: /fixtures/primitives/Rule/.
// Spec: design/layout.md ("Rules (lines)"): hairline 1px rule, ink 1px ink, section 2px ink, brand 1px crimson
// with a 36 x 7px tag at the left.
import { expect, gotoPage, setTheme, test } from "../fixtures";
import { fixtureCase, galleryChecks, galleryPath, tokenRgb } from "./primitives";

const PATH = galleryPath("Rule");

galleryChecks("Rule", 5);

const EXPECTED = {
  hairline: ["1px", "--color-rule"],
  ink: ["1px", "--color-ink"],
  section: ["2px", "--color-ink"],
  brand: ["1px", "--color-brand"],
} as const;

const drawn = (el: Element) => {
  const s = getComputedStyle(el);
  const tag = getComputedStyle(el, "::before");
  const r = el.getBoundingClientRect();
  const parent = (el.parentElement as HTMLElement).getBoundingClientRect();
  return {
    width: s.borderTopWidth,
    style: s.borderTopStyle,
    color: s.borderTopColor,
    tag: tag.content === "none" ? null : [tag.width, tag.height, tag.backgroundColor],
    tagLeft: r.left,
    spans: Math.abs(r.width - parent.width) < 1,
  };
};

test.describe("Rule", () => {
  test("draws each rule at its width and colour, across the column, in both themes", async ({ page, theme }) => {
    await gotoPage(page, PATH);
    for (const t of [theme, theme === "light" ? "dark" : "light"] as const) {
      await setTheme(page, t);
      for (const [kind, [width, color]] of Object.entries(EXPECTED)) {
        const d = await fixtureCase(page, kind).locator(".rule").evaluate(drawn);
        expect([d.width, d.style, d.color], `${kind} in ${t}`).toEqual([width, "solid", tokenRgb(color, t)]);
        expect(d.spans, kind).toBe(true);
        if (kind === "brand") expect(d.tag).toEqual(["36px", "7px", tokenRgb("--color-brand", t)]);
        else expect(d.tag, kind).toBeNull();
      }
      const top = await fixtureCase(page, "brand rule on top of a block").locator(".rule-top").evaluate(drawn);
      expect([top.width, top.color, top.tag]).toEqual([
        "1px",
        tokenRgb("--color-brand", t),
        ["36px", "7px", tokenRgb("--color-brand", t)],
      ]);
    }
  });

  test("the tag sits on the rule at its left end", async ({ page }) => {
    await gotoPage(page, PATH);
    const g = await fixtureCase(page, "brand")
      .locator(".rule")
      .evaluate((el) => {
        const tag = getComputedStyle(el, "::before");
        return { left: tag.left, top: tag.top, insetStart: tag.insetInlineStart };
      });
    expect(g.top).toBe("-1px");
    expect([g.left, g.insetStart]).toContain("0px");
  });

  test("prints the rules and keeps the crimson tag", { tag: "@print" }, async ({ page }) => {
    await gotoPage(page, PATH);
    await page.emulateMedia({ media: "print" });
    const d = await fixtureCase(page, "brand")
      .locator(".rule")
      .evaluate((el) => ({
        ...{ w: getComputedStyle(el, "::before").width },
        adjust:
          getComputedStyle(el, "::before").getPropertyValue("print-color-adjust") ||
          getComputedStyle(el, "::before").getPropertyValue("-webkit-print-color-adjust"),
        visible: getComputedStyle(el).display !== "none",
      }));
    expect(d).toEqual({ w: "36px", adjust: "exact", visible: true });
  });

  test("keeps every rule and the tag visible in forced colours", { tag: "@forced-colors" }, async ({ page }) => {
    await gotoPage(page, PATH);
    for (const kind of Object.keys(EXPECTED)) {
      const d = await fixtureCase(page, kind)
        .locator(".rule")
        .evaluate((el) => ({
          border: getComputedStyle(el).borderTopColor,
          bg: getComputedStyle(document.body).backgroundColor,
          tag: getComputedStyle(el, "::before").backgroundColor,
        }));
      expect(d.border, kind).not.toBe(d.bg);
      expect(d.border, kind).not.toBe("rgba(0, 0, 0, 0)");
      if (kind === "brand") expect(d.tag).not.toBe(d.bg);
    }
  });
});
