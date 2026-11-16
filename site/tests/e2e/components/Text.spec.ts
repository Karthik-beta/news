// Text (the type scale) in browsers, on every screen (news-component-testing). Gallery: /fixtures/primitives/Text/.
// Spec: design/typography.md and tokens.json; phone sizes below 600px, tablet and up from 600px (layout.md).
import { readTokens } from "../../support/tokens";
import { expect, gotoPage, setTheme, test } from "../fixtures";
import { fixtureCase, galleryChecks, galleryPath, tokenRgb } from "./primitives";

const PATH = galleryPath("Text");

galleryChecks("Text", 21);

const px = (name: string) => {
  const t = readTokens().find((x) => x.name === name);
  if (!t) throw new Error(name);
  return Number.parseFloat(t.light) * (t.light.endsWith("rem") ? 16 : 1);
};

/** Fixed sizes from the tokens; responsive ones take the phone value under 600px (tokens.css). */
const FIXED: Record<string, string> = {
  "headline-l": "--font-size-headline-l",
  "headline-m": "--font-size-headline-m",
  "headline-s": "--font-size-headline-s",
  "lead-summary": "--font-size-headline-s",
  "body-s": "--font-size-body-s",
  fly: "--font-size-ui",
  "chart-title": "--font-size-chart-title",
  "chart-subtitle": "--font-size-chart-subtitle",
  "figure-s": "--font-size-figure-s",
  ui: "--font-size-ui",
  label: "--font-size-label",
  source: "--font-size-source",
};
const RESPONSIVE: Record<string, [number, string]> = {
  body: [17, "--font-size-body"],
  standfirst: [18, "--font-size-standfirst"],
  figure: [24, "--font-size-figure"],
  wordmark: [26, "--font-size-wordmark"],
};

test.describe("Text", () => {
  test("every variant has its size for this screen, its face and a line height of at least 1", async ({ page }) => {
    await gotoPage(page, PATH);
    const width = page.viewportSize()?.width ?? 0;
    const styles: Record<string, { size: number; lh: number; family: string }> = await page
      .locator(".fixture-case")
      .evaluateAll((cases) =>
        Object.fromEntries(
          cases.map((c) => {
            const el = c.querySelector("[class^='text-']:not(.fixture-case__name)") as HTMLElement;
            const s = getComputedStyle(el);
            return [
              c.getAttribute("aria-label"),
              { size: Number.parseFloat(s.fontSize), lh: Number.parseFloat(s.lineHeight), family: s.fontFamily },
            ];
          }),
        ),
      );
    for (const [variant, token] of Object.entries(FIXED)) {
      expect(styles[variant]?.size, variant).toBeCloseTo(px(token), 1);
    }
    for (const [variant, [phone, token]] of Object.entries(RESPONSIVE)) {
      expect(styles[variant]?.size, variant).toBeCloseTo(width < 600 ? phone : px(token), 1);
    }
    for (const variant of ["display", "headline-xl"]) {
      // Fluid sizes: between the body size and the largest display size.
      expect(styles[variant]?.size, variant).toBeGreaterThan(20);
      expect(styles[variant]?.size, variant).toBeLessThan(64);
    }
    for (const [variant, s] of Object.entries(styles)) {
      expect(s.lh / s.size, `${variant} line height`).toBeGreaterThanOrEqual(0.99);
    }
    // Reading text keeps a line height of at least 1.5 (accessibility.md).
    expect((styles.body?.lh ?? 0) / (styles.body?.size ?? 1)).toBeGreaterThanOrEqual(1.5);
    expect(styles.body?.family).toMatch(/Literata/);
    expect(styles.label?.family).toMatch(/Fira Sans/);
  });

  test("colours come from the tokens in both themes: crimson only for the wordmark and fly title", async ({
    page,
    theme,
  }) => {
    await gotoPage(page, PATH);
    for (const t of [theme, theme === "light" ? "dark" : "light"] as const) {
      await setTheme(page, t);
      const color = (name: string) =>
        fixtureCase(page, name)
          .locator("[class^='text-']:not(.fixture-case__name)")
          .first()
          .evaluate((el) => getComputedStyle(el).color);
      expect(await color("wordmark")).toBe(tokenRgb("--color-brand", t));
      expect(await color("fly")).toBe(tokenRgb("--color-brand", t));
      expect(await color("chart-subtitle")).toBe(tokenRgb("--color-ink-2", t));
      expect(await color("label")).toBe(tokenRgb("--color-ink-2", t));
      expect(await color("source")).toBe(tokenRgb("--color-ink-3", t));
      for (const name of ["body", "headline-xl", "figure", "chart-title"]) {
        expect(await color(name), name).toBe(tokenRgb("--color-ink", t));
      }
    }
  });

  test("a long headline wraps inside the column, and a long word or URL breaks instead of overflowing", async ({
    page,
  }) => {
    await gotoPage(page, PATH);
    for (const name of ["long headline", "long unbroken word"]) {
      const el = fixtureCase(page, name).locator("[class^='text-']:not(.fixture-case__name)");
      const g = await el.evaluate((e) => ({
        scroll: e.scrollWidth,
        client: e.clientWidth,
        right: e.getBoundingClientRect().right,
        parentRight: (e.parentElement as HTMLElement).getBoundingClientRect().right,
        lines: Math.round(e.getBoundingClientRect().height / Number.parseFloat(getComputedStyle(e).lineHeight)),
      }));
      expect(g.scroll, name).toBeLessThanOrEqual(g.client + 1);
      expect(g.right, name).toBeLessThanOrEqual(g.parentRight + 0.5);
      expect(g.lines, `${name} wraps`).toBeGreaterThan(1);
    }
  });

  test("tabular figures line up and use the true minus", async ({ page }) => {
    await gotoPage(page, PATH);
    const el = fixtureCase(page, "tabular figures").locator(".text-tabular");
    await expect(el).toHaveText("1,111.1 / 999.9 / −5.7");
    expect(await el.evaluate((e) => getComputedStyle(e).fontVariantNumeric)).toMatch(/tabular-nums/);
  });
});
