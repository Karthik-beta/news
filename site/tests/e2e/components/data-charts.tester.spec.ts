// Component tester's browser checks for #8 (data components and Charts I), on top of the builder's
// data-charts.spec.ts: every fixture page on every screen, both themes and three engines — nothing clipped or off
// screen, 200% zoom and text, fonts blocked, island scripts blocked, no motion, no shift once the fonts are in,
// forced colours and print; popovers open on click, tap, Enter or Space only (Karthik, 16 Nov 2026); one tab stop
// per chart; hover tooltips; the data table's fixed first column; rates keep the source's digits; crimson never
// marks a subject that fell; and the components in context on the charts article fixture.
import { tokenValue } from "../../support/tokens";
import {
  cumulativeLayoutShift,
  expect,
  expectAccessible,
  expectNoSidewaysScroll,
  gotoPage,
  longestMotionMs,
  runningAnimations,
  setTextSize,
  setTheme,
  test,
  watchLayoutShift,
  zoomTo200,
} from "../fixtures";
import { expectInsideViewport, expectNoClippedText, galleryChecks, panelFor, rgb } from "./primitives";

type Page = import("@playwright/test").Page;

const PAGES = [
  "KeyFigure/default",
  "KeyFigures/default",
  "KeyFigures/edge-cases",
  "DataTable/default",
  "DataTable/edge-cases",
  "ComparisonTable/default",
  "ComparisonTable/edge-cases",
  "ColumnChart/default",
  "ColumnChart/edge-cases",
  "BarChart/default",
  "BarChart/edge-cases",
  "LineChart/default",
  "LineChart/edge-cases",
  "StepLineChart/default",
  "StepLineChart/edge-cases",
];
const url = (p: string) => `/fixtures/components/${p}/`;
/** Popover panels and chart tooltips position themselves; they are checked by their own tests. */
const FLOATING = "[popover], .popover--tooltip";

async function hydrateAll(page: Page): Promise<void> {
  const islands = page.locator("[data-island]");
  for (let i = 0; i < (await islands.count()); i++) {
    await islands.nth(i).scrollIntoViewIfNeeded();
    await expect(islands.nth(i).locator("[data-hydrated='true']")).toHaveCount(1);
  }
}

const figureOf = (page: Page, statement: string) => page.locator(`figure:has([id="${statement}-title"])`);

// The primitives #8 added, through the same gallery checks #4's primitives passed.
galleryChecks("ChartFigure", 4);
galleryChecks("DataLinks", 3);
galleryChecks("SourcePopover", 5);
galleryChecks("Sparkline", 8);

for (const p of PAGES) {
  test(`${p}: nothing clipped or off screen once every island has hydrated`, async ({ page }) => {
    await gotoPage(page, url(p));
    await hydrateAll(page);
    await expectNoSidewaysScroll(page);
    await expectInsideViewport(page, page.locator("main"), FLOATING);
    await expectNoClippedText(page);
  });

  test(`${p}: reflows at 200% text size and 200% zoom`, { tag: "@zoom" }, async ({ page }) => {
    await gotoPage(page, url(p));
    await hydrateAll(page);
    await setTextSize(page, 200);
    await expectNoSidewaysScroll(page);
    await expectInsideViewport(page, page.locator("main"), FLOATING);
    await setTextSize(page, 100);
    test.skip(!(await zoomTo200(page)), "below 640px wide, 200% zoom is the 320 project (400%)");
    await expectNoSidewaysScroll(page);
    await expectInsideViewport(page, page.locator("main"), FLOATING);
  });

  test(`${p}: nothing animates on load and nothing moves once the fonts are in`, async ({ page }) => {
    await gotoPage(page, url(p));
    const shift = await watchLayoutShift(page);
    await hydrateAll(page);
    expect(await runningAnimations(page)).toEqual([]);
    expect(await longestMotionMs(page), "no motion under reduced motion").toBe(0);
    const total = await cumulativeLayoutShift(page);
    if (total !== null) {
      test.info().annotations.push({ type: "CLS since navigation, font swap included", description: String(total) });
      expect(await shift(), "layout shift after the fonts loaded").toBeLessThan(0.001);
    }
  });

  test(`${p}: transitions stay within 200 ms when motion is allowed`, { tag: "@motion" }, async ({ page }) => {
    await gotoPage(page, url(p));
    await hydrateAll(page);
    expect(await runningAnimations(page)).toEqual([]);
    expect(await longestMotionMs(page)).toBeLessThanOrEqual(200);
  });

  test(`${p}: passes axe in forced colours and keeps every mark visible`, { tag: "@forced-colors" }, async ({
    page,
  }) => {
    await gotoPage(page, url(p));
    await hydrateAll(page);
    expect(await page.evaluate(() => matchMedia("(forced-colors: active)").matches)).toBe(true);
    await expectAccessible(page);
    const invisible = await page.locator(".bar-chart__bar, .column-chart__fill, .chart-key").evaluateAll((els) =>
      els
        .filter((el) => {
          const s = getComputedStyle(el);
          const r = el.getBoundingClientRect();
          return (
            r.width > 0 &&
            r.height > 0 &&
            s.backgroundColor === "rgba(0, 0, 0, 0)" &&
            s.backgroundImage === "none" && // hatching for missing data is a background image
            s.borderTopWidth === "0px"
          );
        })
        .map((el) => el.className),
    );
    expect(invisible, "marks drawn in forced colours").toEqual([]);
  });

  test(`${p}: prints without the interface`, { tag: "@print" }, async ({ page }) => {
    await gotoPage(page, url(p));
    await hydrateAll(page);
    await page.emulateMedia({ media: "print" });
    for (const actions of await page.locator(".figure-frame__actions, .data-links").all()) {
      await expect(actions).toBeHidden();
    }
    for (const t of await page.locator(".popover-trigger").all()) {
      expect(await t.evaluate((el) => getComputedStyle(el).textDecorationLine), "no tap underline on paper").toBe(
        "none",
      );
    }
    for (const fig of await page.locator("figure").all()) await expect(fig.locator("figcaption")).toBeVisible();
  });
}

test.describe(() => {
  // Blocking the island scripts is the failure under test.
  test.use({
    allowedConsoleErrors: [
      [/^Failed to load resource/, /request failed: .*\/_astro\//, /dynamically imported/i],
      {
        scope: "test",
      },
    ] as never,
  });
  test("charts still read when their island scripts fail to load", async ({ page }) => {
    await page.route("**/_astro/**/*.js", (r) => r.abort());
    await page.goto(url("BarChart/default"));
    const fig = page.locator("figure").first();
    await expect(fig.locator(".bar-chart__bar").first()).toBeVisible();
    await expect(fig.locator(".bar-chart__value").first()).toBeVisible();
    expect(await page.locator("[role='img'][tabindex]").count(), "no dead tab stop").toBe(0);
    await fig.getByText("Show the data table").click();
    await expect(fig.locator("tbody tr")).toHaveCount(5);
    await expectNoSidewaysScroll(page);
  });
});

test("missing values are hatched bars and columns, never empty boxes (DC-3)", async ({ page }) => {
  for (const p of ["ColumnChart/edge-cases", "BarChart/edge-cases"]) {
    await gotoPage(page, url(p));
    await hydrateAll(page);
    const hatches = page.locator(".chart-hatch");
    expect(await hatches.count(), p).toBeGreaterThan(0);
    for (const h of await hatches.all()) {
      expect(await h.evaluate((el) => getComputedStyle(el).backgroundImage), p).toContain("repeating-linear-gradient");
    }
  }
});

test.describe("citation popovers open on click, tap, Enter or Space only", () => {
  test("hover and focus leave it closed; Enter, Space and click open it; Escape and outside close it", {
    tag: ["@interaction", "@smoke"],
  }, async ({ page, isMobile }) => {
    await gotoPage(page, url("KeyFigures/default"));
    const trigger = page.locator(".key-figure__figure .popover-trigger").first();
    const panel = await panelFor(page, trigger);
    const open = () => panel.evaluate((el) => el.matches(":popover-open"));
    if (!isMobile) {
      await trigger.hover();
      await page.waitForTimeout(400);
      expect(await open(), "hover does not open").toBe(false);
    }
    await trigger.focus();
    await page.waitForTimeout(200);
    expect(await open(), "focus does not open").toBe(false);
    await page.keyboard.press("Enter");
    await expect.poll(open).toBe(true);
    await expect(panel.locator(".popover__value")).toHaveText("$391,035 million");
    await page.keyboard.press("Escape");
    await expect.poll(open).toBe(false);
    await trigger.focus();
    await page.keyboard.press(" ");
    await expect.poll(open).toBe(true);
    await page.keyboard.press("Escape");
    await expect.poll(open).toBe(false);
    await trigger.click();
    await expect.poll(open).toBe(true);
    await page.mouse.click(2, 2);
    await expect.poll(open).toBe(false);
  });

  test("a tap opens it and a tap outside closes it", { tag: "@interaction" }, async ({ page, hasTouch }) => {
    test.skip(!hasTouch, "touch screens only");
    await gotoPage(page, url("KeyFigures/default"));
    const trigger = page.locator(".key-figure__figure .popover-trigger").first();
    const panel = await panelFor(page, trigger);
    const box = await trigger.boundingBox();
    if (!box) throw new Error("no trigger box");
    await page.touchscreen.tap(box.x + box.width / 2, box.y + box.height / 2);
    await expect.poll(() => panel.evaluate((el) => el.matches(":popover-open"))).toBe(true);
    const pb = await panel.boundingBox();
    const vw = page.viewportSize()?.width ?? 0;
    expect(pb && pb.x >= 0 && pb.x + pb.width <= vw + 0.5, "panel wholly on screen").toBe(true);
    await page.touchscreen.tap(2, 2);
    await expect.poll(() => panel.evaluate((el) => el.matches(":popover-open"))).toBe(false);
  });
});

test.describe("chart keyboard and pointer", () => {
  for (const p of ["ColumnChart/default", "BarChart/default", "LineChart/default", "StepLineChart/default"]) {
    test(`${p}: each chart is one tab stop`, { tag: "@interaction" }, async ({ page }) => {
      await gotoPage(page, url(p));
      await hydrateAll(page);
      const plot = page.locator("[data-island]").first().getByRole("img");
      await plot.focus();
      await page.keyboard.press("ArrowRight");
      await page.keyboard.press("Tab");
      const stillInside = await page
        .locator("[data-island]")
        .first()
        .evaluate((island) => island.contains(document.activeElement));
      expect(stillInside, "Tab leaves the plot after one stop").toBe(false);
      expect(
        await page.locator("[data-island]").first().locator("[tabindex='0'], button, a[href]").count(),
        "one focusable element per plot",
      ).toBe(1);
    });
  }

  for (const { p, mark, value } of [
    { p: "ColumnChart/default", mark: "[data-band='4']", value: "$96.2bn" },
    { p: "BarChart/default", mark: "[data-row='0']", value: "$167.0bn" },
  ]) {
    test(`${p}: hovering a mark shows its tooltip, wholly on screen`, { tag: "@interaction" }, async ({
      page,
      isMobile,
    }) => {
      test.skip(isMobile, "no hover on touch screens");
      await gotoPage(page, url(p));
      const island = page.locator("[data-island]").first();
      await island.scrollIntoViewIfNeeded();
      await expect(island.locator("[data-hydrated='true']")).toHaveCount(1);
      const box = await island.locator(mark).first().boundingBox();
      if (!box) throw new Error("no mark box");
      await page.mouse.move(box.x + box.width / 2, box.y + box.height - 3);
      const tip = island.locator(".chart-tooltip");
      await expect(tip.locator(".popover__value").first()).toHaveText(value);
      const tb = await tip.boundingBox();
      const vw = page.viewportSize()?.width ?? 0;
      expect(tb && tb.x >= -0.5 && tb.x + tb.width <= vw + 0.5, "tooltip on screen").toBe(true);
    });
  }

  test("the last point's tooltip stays on screen at every width", { tag: "@interaction" }, async ({ page }) => {
    test.setTimeout(180_000);
    for (const p of ["ColumnChart/default", "LineChart/default", "StepLineChart/default", "BarChart/default"]) {
      await gotoPage(page, url(p));
      const island = page.locator("[data-island]").first();
      await island.scrollIntoViewIfNeeded();
      await expect(island.locator("[data-hydrated='true']")).toHaveCount(1);
      await island.getByRole("img").focus();
      await page.keyboard.press("End");
      const tb = await island.locator(".chart-tooltip").boundingBox();
      const vw = page.viewportSize()?.width ?? 0;
      expect(tb && tb.x >= -0.5 && tb.x + tb.width <= vw + 0.5, `${p}: tooltip on screen`).toBe(true);
      await hydrateAll(page);
    }
  });
});

test("rates keep the source's digits in step-line tooltips, tables and key figures", { tag: "@smoke" }, async ({
  page,
}) => {
  await gotoPage(page, url("StepLineChart/default"));
  const island = page.locator("[data-island]").first();
  await island.scrollIntoViewIfNeeded();
  await expect(island.locator("[data-hydrated='true']")).toHaveCount(1);
  await island.getByRole("img").focus();
  await page.keyboard.press("Home");
  const tip = island.locator(".chart-tooltip .popover__value").first();
  await expect(tip).toHaveText("0.25%");
  await page.keyboard.press("ArrowRight");
  await expect(tip).toHaveText("0.50%");
  await page.keyboard.press("ArrowRight");
  await expect(tip).toHaveText("1.00%");
  const fig = page.locator("figure").first();
  await fig.getByText("Show the data table").click();
  await expect(fig.locator("tbody tr").nth(1).locator("td")).toHaveText("0.50");
  await hydrateAll(page);

  await gotoPage(page, url("KeyFigures/edge-cases"));
  const rates = page.locator(".key-figures", { hasText: "Federal funds rate at its peak" });
  await expect(rates.locator(".key-figure__figure .popover-trigger")).toHaveText(["5.50%", "4.50%"]);
});

test("the data table's first column stays put while the table scrolls sideways", { tag: "@interaction" }, async ({
  page,
}) => {
  await gotoPage(page, url("DataTable/edge-cases"));
  const scrollers = page.locator(".data-table__scroll");
  let checked = 0;
  for (const s of await scrollers.all()) {
    const overflow = await s.evaluate((el) => el.scrollWidth > el.clientWidth + 1);
    if (!overflow) continue;
    const th = s.locator("tbody th").first();
    expect(await th.evaluate((el) => getComputedStyle(el).position)).toBe("sticky");
    const before = (await th.boundingBox())?.x;
    await s.evaluate((el) => {
      el.scrollLeft = el.scrollWidth;
    });
    const after = (await th.boundingBox())?.x;
    expect(after, "first column did not move").toBeCloseTo(before ?? -1, 0);
    checked++;
  }
  test.info().annotations.push({ type: "wide tables scrolled", description: String(checked) });
});

test.describe("crimson never marks a highlighted subject that fell (Karthik, 16 Nov 2026; DC-1)", () => {
  const CASES = [
    { p: "BarChart/edge-cases", statement: "fell", marks: ".bar-chart__row:has(.is-negative) .bar-chart__bar" },
    { p: "ColumnChart/edge-cases", statement: "fell", marks: ".column-chart__fill" },
    {
      p: "LineChart/edge-cases",
      statement: "fell",
      marks: ".line-chart__line, .line-chart__marker, .line-chart__end-label",
    },
  ];
  for (const { p, statement, marks } of CASES) {
    test(`${p} "${statement}": no crimson anywhere in the plot, in either theme`, { tag: "@smoke" }, async ({
      page,
      theme,
    }) => {
      await gotoPage(page, url(p));
      await hydrateAll(page);
      const fig = figureOf(page, statement);
      for (const t of [theme, theme === "light" ? "dark" : "light"] as const) {
        await setTheme(page, t);
        const crimson = rgb(tokenValue("--chart-highlight", t));
        const painted = await fig
          .locator(`.chart-plot ${marks}, .chart-plot .is-highlight, [data-island] ${marks}`)
          .evaluateAll((els) =>
            els.flatMap((el) => {
              const s = getComputedStyle(el);
              return [s.backgroundColor, s.color, s.stroke, s.fill];
            }),
          );
        expect(painted.length, "marks found").toBeGreaterThan(0);
        expect(painted, `${t}: the fallen subject is not crimson`).not.toContain(crimson);
      }
    });
  }
});

test.describe("in context: the charts article fixture", () => {
  const PATH = "/fixtures/renderer/charts/";
  test("every block reads at this size, in order and without overlapping", async ({ page }) => {
    await gotoPage(page, PATH);
    await hydrateAll(page);
    await expectNoSidewaysScroll(page);
    await expectInsideViewport(page, page.locator("main"), FLOATING);
    await expectNoClippedText(page);
    const blocks = await page
      .locator("article .key-figures, article figure")
      .evaluateAll((els) => els.map((el) => el.getBoundingClientRect()).map((r) => ({ top: r.top, bottom: r.bottom })));
    expect(blocks.length).toBeGreaterThanOrEqual(9);
    for (let i = 1; i < blocks.length; i++) {
      const gap = (blocks[i]?.top ?? 0) - (blocks[i - 1]?.bottom ?? 0);
      expect(gap, `space between block ${i} and ${i + 1}`).toBeGreaterThanOrEqual(16);
    }
  });

  test("passes axe in light and dark with every island hydrated", { tag: "@a11y" }, async ({ page, theme }) => {
    await gotoPage(page, PATH);
    await hydrateAll(page);
    await expectAccessible(page);
    await setTheme(page, theme === "light" ? "dark" : "light");
    await expectAccessible(page);
  });
});
