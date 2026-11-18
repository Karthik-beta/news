// FigureFrame in browsers, on every screen (news-component-testing). Gallery: /fixtures/primitives/FigureFrame/;
// in context: the chart on /fixtures/renderer/apple-fy2024/. Spec: design/charts.md ("Anatomy").
import { expect, expectAccessible, expectNoSidewaysScroll, gotoPage, setTheme, test } from "../fixtures";
import { expectInsideViewport, fixtureCase, galleryChecks, galleryPath, tokenRgb } from "./primitives";

const PATH = galleryPath("FigureFrame");

galleryChecks("FigureFrame", 6);

test.describe("FigureFrame", () => {
  test("spaces the anatomy as charts.md says: 16, 4, 12, 10 and 6px", async ({ page }) => {
    await gotoPage(page, PATH);
    const g = await fixtureCase(page, "full anatomy")
      .locator("figure")
      .evaluate((fig) => {
        const box = (sel: string) => (fig.querySelector(sel) as HTMLElement).getBoundingClientRect();
        const figure = fig.getBoundingClientRect();
        const border = Number.parseFloat(getComputedStyle(fig).borderTopWidth);
        return {
          ruleToTitle: box(".figure-frame__title").top - figure.top - border,
          titleToSubtitle: box(".figure-frame__subtitle").top - box(".figure-frame__title").bottom,
          subtitleToLegend: box(".figure-frame__legend").top - box(".figure-frame__caption").bottom,
          plotToSource: box(".figure-frame__source").top - box(".figure-frame__plot").bottom,
          sourceToActions: box(".figure-frame__actions").top - box(".figure-frame__source").bottom,
        };
      });
    expect(g.ruleToTitle).toBeCloseTo(16, 0);
    expect(g.titleToSubtitle).toBeCloseTo(4, 0);
    expect(g.subtitleToLegend).toBeCloseTo(12, 0);
    expect(g.plotToSource).toBeCloseTo(10, 0);
    expect(g.sourceToActions).toBeCloseTo(6, 0);
  });

  test("draws the brand rule and the 36 x 7px tag in crimson, in both themes", async ({ page, theme }) => {
    await gotoPage(page, PATH);
    const figure = fixtureCase(page, "full anatomy").locator("figure");
    for (const t of [theme, theme === "light" ? "dark" : "light"] as const) {
      await setTheme(page, t);
      const s = await figure.evaluate((fig) => {
        const cs = getComputedStyle(fig);
        const tag = getComputedStyle(fig, "::before");
        return {
          width: cs.borderTopWidth,
          style: cs.borderTopStyle,
          color: cs.borderTopColor,
          tag: [tag.width, tag.height, tag.backgroundColor],
          figureWidth: fig.getBoundingClientRect().width,
          parentWidth: (fig.parentElement as HTMLElement).clientWidth,
        };
      });
      expect(s.width).toBe("1px");
      expect(s.style).toBe("solid");
      expect(s.color).toBe(tokenRgb("--color-brand", t));
      expect(s.tag).toEqual(["36px", "7px", tokenRgb("--color-brand", t)]);
      expect(s.figureWidth, "the rule spans the column").toBeCloseTo(s.parentWidth, 0);
    }
  });

  test("sets the title, subtitle, source and actions in the chart type styles", async ({ page, theme }) => {
    await gotoPage(page, PATH);
    const f = fixtureCase(page, "full anatomy");
    const style = (sel: string) =>
      f
        .locator(sel)
        .first()
        .evaluate((el) => {
          const s = getComputedStyle(el);
          return { size: s.fontSize, weight: s.fontWeight, color: s.color, family: s.fontFamily };
        });
    const title = await style(".figure-frame__title");
    expect([title.size, title.weight]).toEqual(["18px", "700"]);
    expect(title.family).toMatch(/Fira Sans/);
    const subtitle = await style(".figure-frame__subtitle");
    expect([subtitle.size, subtitle.color]).toEqual(["14.5px", tokenRgb("--color-ink-2", theme)]);
    const source = await style(".figure-frame__source");
    expect([source.size, source.color]).toEqual(["12.5px", tokenRgb("--color-ink-3", theme)]);
    for (const sel of [".figure-frame__actions > a", ".figure-frame__table > summary"]) {
      const a = await style(sel);
      expect([a.size, a.color], sel).toEqual(["13.5px", tokenRgb("--color-ink-2", theme)]);
    }
  });

  test("titles the figure (h2 by default, h3 when asked) and links the source line to the sources list", async ({
    page,
  }) => {
    await gotoPage(page, PATH);
    await expect(
      page.getByRole("figure", { name: "Services earns about twice the gross margin of products" }),
    ).toBeVisible();
    await expect(
      fixtureCase(page, "full anatomy").getByRole("heading", { level: 2, name: /Services earns/ }),
    ).toBeVisible();
    await expect(
      fixtureCase(page, "level-3 title, JSON only, unbroken source name").getByRole("heading", { level: 3 }),
    ).toBeVisible();
    const link = fixtureCase(page, "full anatomy").locator(".figure-frame__source a");
    const target = (await link.getAttribute("href")) as string;
    await expect(page.locator(target)).toHaveCount(1);
  });

  test("opens and closes the data table from the keyboard", { tag: "@interaction" }, async ({ page }) => {
    await gotoPage(page, PATH);
    const f = fixtureCase(page, "full anatomy");
    const summary = f.locator("summary");
    const table = f.locator(".figure-frame__table-scroll table");
    await expect(table).toBeHidden();
    await summary.focus();
    const ring = await summary.evaluate((el) => {
      const s = getComputedStyle(el);
      return [s.outlineStyle, s.outlineWidth];
    });
    expect(ring, "visible focus ring").toEqual(["solid", "2px"]);
    await page.keyboard.press("Enter");
    await expect(table).toBeVisible();
    await expect(table.locator("tbody tr")).toHaveCount(1);
    await page.keyboard.press(" ");
    await expect(table).toBeHidden();
  });

  test("opens the data table on tap", { tag: "@interaction" }, async ({ page, hasTouch }) => {
    test.skip(!hasTouch, "touch screens only");
    await gotoPage(page, PATH);
    const f = fixtureCase(page, "full anatomy");
    await f.locator("summary").tap();
    await expect(f.locator(".figure-frame__table-scroll table")).toBeVisible();
  });

  test("a wide data table scrolls inside the frame, never the page", { tag: "@interaction" }, async ({ page }) => {
    await gotoPage(page, PATH);
    const f = fixtureCase(page, "wide data table scrolls inside the frame");
    await f.locator("summary").click();
    const scroller = f.locator(".figure-frame__table-scroll");
    await expect(scroller.locator("table")).toBeVisible();
    await expectNoSidewaysScroll(page);
    await expectInsideViewport(page);
    const { scrollWidth, clientWidth } = await scroller.evaluate((el) => ({
      scrollWidth: el.scrollWidth,
      clientWidth: el.clientWidth,
    }));
    test.info().annotations.push({ type: "table overflow", description: `${scrollWidth} in ${clientWidth}` });
    expect(clientWidth).toBeLessThanOrEqual(page.viewportSize()?.width ?? 0);
  });

  test("an open wide data table stays accessible (a scrolling region keyboard users can reach)", {
    tag: ["@a11y", "@interaction"],
  }, async ({ page }) => {
    await gotoPage(page, PATH);
    for (const f of await page.locator(".fixture-case").all()) {
      const summary = f.locator("summary");
      if ((await summary.count()) > 0) await summary.click();
    }
    await expectAccessible(page);
  });

  test("hides the actions in print and keeps the figure, the title, the source line and the tag", {
    tag: "@print",
  }, async ({ page }) => {
    await gotoPage(page, PATH);
    await page.emulateMedia({ media: "print" });
    const f = fixtureCase(page, "full anatomy");
    await expect(f.locator(".figure-frame__actions")).toBeHidden();
    await expect(f.locator(".figure-frame__title")).toBeVisible();
    await expect(f.locator(".figure-frame__source")).toBeVisible();
    const tag = await f.locator("figure").evaluate((fig) => {
      const s = getComputedStyle(fig, "::before");
      return {
        w: s.width,
        adjust: s.getPropertyValue("print-color-adjust") || s.getPropertyValue("-webkit-print-color-adjust"),
      };
    });
    expect(tag.w).toBe("36px");
    expect(tag.adjust).toBe("exact");
  });

  test("keeps the rule and the tag in forced colours", { tag: "@forced-colors" }, async ({ page }) => {
    await gotoPage(page, PATH);
    const s = await fixtureCase(page, "full anatomy")
      .locator("figure")
      .evaluate((fig) => ({
        border: getComputedStyle(fig).borderTopColor,
        tag: getComputedStyle(fig, "::before").backgroundColor,
        text: getComputedStyle(fig).color,
      }));
    expect(s.tag).not.toBe("rgba(0, 0, 0, 0)");
    expect(s.tag, "the tag is drawn in the text colour").toBe(s.text);
    expect(s.border).not.toBe("rgba(0, 0, 0, 0)");
  });

  test("in context: the chart on the article fixture page has the same anatomy and fits the column", async ({
    page,
  }) => {
    await gotoPage(page, "/fixtures/renderer/apple-fy2024/");
    const figure = page.locator("figure.figure-frame").first();
    await expect(figure).toBeVisible();
    await expect(figure.locator("figcaption .figure-frame__title")).toBeVisible();
    await expectNoSidewaysScroll(page);
    await expectInsideViewport(page);
    const gap = await figure.evaluate((fig) => {
      const prev = fig.closest(".page-body > *")?.previousElementSibling;
      return prev ? fig.getBoundingClientRect().top - prev.getBoundingClientRect().bottom : null;
    });
    if (gap !== null) expect(gap, "28px from the block before it (layout.md)").toBeCloseTo(28, 0);
  });
});
