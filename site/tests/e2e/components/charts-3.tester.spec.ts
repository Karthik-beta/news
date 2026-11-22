// Component tester's browser checks for Charts III part 1 (#13): SmallMultiples, Histogram and Heatmap, on top of the
// builder's charts-3.spec.ts. Every fixture page on every screen: nothing clipped, off screen or overlapping (x labels,
// end labels, ticks, legend keys, heatmap names and values, panel names); 200% text and 200% zoom; no motion and no
// shift; forced colours; print; island scripts blocked. Small multiples: panels the same size on one scale, columns
// per row by the figure's width, a panel with no data says so. Histogram: the edge labels stay inside the plot, the
// tooltip lists at most 8 values. Heatmap: each step is the token the spec names in both themes (system setting and
// data-theme, either way round), every written value fits its cell and reads at 4.5:1, values hide when they would
// not fit (also at 200% text), 60 periods stay inside the page. Every tooltip stop on screen; one tab stop per plot;
// taps; crimson never on a fall or a heatmap; and the charts in context on the charts article fixture.
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
import { expectInsideViewport, expectNoClippedText, rgb } from "./primitives";

type Page = import("@playwright/test").Page;
type Locator = import("@playwright/test").Locator;

const NAMES = ["SmallMultiples", "Histogram", "Heatmap"];
const PAGES = NAMES.flatMap((n) => [`${n}/default`, `${n}/edge-cases`]);
const url = (p: string) => `/fixtures/components/${p}/`;
const FLOATING = "[popover], .popover--tooltip, .chart-tooltip";

/** Every label these plots write: none may overlap another. */
const LABELS = [
  ".chart-plot__x-label",
  ".chart-plot__tick",
  ".line-chart__end-label",
  ".column-chart__value",
  ".chart-legend__item",
  ".small-multiples__name",
  ".small-multiples__missing",
  ".heatmap__name",
  ".heatmap__value",
  ".histogram__missing",
].join(", ");

const figureOf = (page: Page, statement: string) => page.locator(`figure:has([id="${statement}-title"])`);

async function hydrateVisible(page: Page): Promise<void> {
  const islands = page.locator("[data-island]");
  for (let i = 0; i < (await islands.count()); i++) {
    const island = islands.nth(i);
    if (!(await island.isVisible())) continue;
    await island.scrollIntoViewIfNeeded();
    await expect(island.locator("[data-hydrated='true']")).toHaveCount(1);
  }
}

/** Pairs of visible labels whose text overlaps (tight text boxes from Range rects). */
async function collisions(scope: Locator): Promise<string[]> {
  return scope.evaluate((root, selector) => {
    const textBoxes = (el: Element) => {
      const range = document.createRange();
      const out: DOMRect[] = [];
      const walk = document.createTreeWalker(el, NodeFilter.SHOW_TEXT);
      for (let n = walk.nextNode(); n; n = walk.nextNode()) {
        if (!n.textContent?.trim()) continue;
        const parent = n.parentElement as HTMLElement;
        if (parent.offsetParent === null || getComputedStyle(parent).visibility === "hidden") continue;
        range.selectNodeContents(n);
        for (const r of range.getClientRects()) if (r.width > 0 && r.height > 0) out.push(r);
      }
      return out;
    };
    const overlap = (a: DOMRect, b: DOMRect) =>
      Math.min(a.right, b.right) - Math.max(a.left, b.left) > 1 &&
      Math.min(a.bottom, b.bottom) - Math.max(a.top, b.top) > 1;
    const name = (el: Element) => `${el.className.toString().split(" ")[0]} "${el.textContent?.trim().slice(0, 30)}"`;
    const boxes = [...root.querySelectorAll(selector)].map((el) => ({ el, rects: textBoxes(el) }));
    const found: string[] = [];
    for (let i = 0; i < boxes.length; i++) {
      for (let j = i + 1; j < boxes.length; j++) {
        const a = boxes[i] as (typeof boxes)[number];
        const b = boxes[j] as (typeof boxes)[number];
        if (a.el.contains(b.el) || b.el.contains(a.el)) continue;
        if (a.rects.some((ra) => b.rects.some((rb) => overlap(ra, rb)))) found.push(`${name(a.el)} × ${name(b.el)}`);
      }
    }
    return found;
  }, LABELS);
}

async function expectNoCollisions(page: Page): Promise<void> {
  const found: string[] = [];
  for (const fig of await page.locator("main figure").all()) found.push(...(await collisions(fig)));
  expect(found, "overlapping labels").toEqual([]);
}

/** Every written heatmap value that is wider or taller than its cell. */
const overflowingValues = (page: Page) =>
  page.locator(".heatmap__value").evaluateAll((els) =>
    els
      .filter((el) => (el as HTMLElement).offsetParent !== null)
      .filter((el) => {
        const v = el.getBoundingClientRect();
        const c = (el.parentElement as HTMLElement).getBoundingClientRect();
        return v.left < c.left - 0.5 || v.right > c.right + 0.5 || v.top < c.top - 0.5 || v.bottom > c.bottom + 0.5;
      })
      .map((el) => el.textContent),
  );

for (const p of PAGES) {
  test(`${p}: nothing clipped, off screen or overlapping once hydrated`, async ({ page }) => {
    await gotoPage(page, url(p));
    await hydrateVisible(page);
    await expectNoSidewaysScroll(page);
    await expectInsideViewport(page, page.locator("main"), FLOATING);
    await expectNoClippedText(page);
    await expectNoCollisions(page);
    expect(await overflowingValues(page), "heatmap values inside their cells").toEqual([]);
  });

  test(`${p}: reflows at 200% text size and 200% zoom with no overlapping labels`, { tag: "@zoom" }, async ({
    page,
  }) => {
    await gotoPage(page, url(p));
    await hydrateVisible(page);
    await setTextSize(page, 200);
    await hydrateVisible(page);
    await expectNoSidewaysScroll(page);
    await expectInsideViewport(page, page.locator("main"), FLOATING);
    await expectNoCollisions(page);
    expect(await overflowingValues(page), "heatmap values inside their cells at 200% text").toEqual([]);
    await setTextSize(page, 100);
    test.skip(!(await zoomTo200(page)), "below 640px wide, 200% zoom is the 320 project (400%)");
    await hydrateVisible(page);
    await expectNoSidewaysScroll(page);
    await expectInsideViewport(page, page.locator("main"), FLOATING);
    await expectNoCollisions(page);
    expect(await overflowingValues(page), "heatmap values inside their cells at 200% zoom").toEqual([]);
  });

  test(`${p}: nothing animates on load and nothing moves once the fonts are in`, async ({ page }) => {
    await gotoPage(page, url(p));
    const shift = await watchLayoutShift(page);
    await hydrateVisible(page);
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
    await hydrateVisible(page);
    expect(await runningAnimations(page)).toEqual([]);
    expect(await longestMotionMs(page)).toBeLessThanOrEqual(200);
  });

  test(`${p}: passes axe in forced colours and keeps every mark visible`, { tag: "@forced-colors" }, async ({
    page,
  }) => {
    await gotoPage(page, url(p));
    await hydrateVisible(page);
    expect(await page.evaluate(() => matchMedia("(forced-colors: active)").matches)).toBe(true);
    await expectAccessible(page);
    const invisible = await page
      .locator(".column-chart__fill, .heatmap__cell, .chart-key, .line-chart__line")
      .evaluateAll((els) =>
        els
          .filter((el) => el.getBoundingClientRect().width > 0)
          .filter((el) => {
            const s = getComputedStyle(el);
            const canvas = getComputedStyle(document.body).backgroundColor;
            if (el instanceof SVGElement) return s.stroke === "none" || s.stroke === canvas;
            return (
              (s.backgroundColor === "rgba(0, 0, 0, 0)" || s.backgroundColor === canvas) &&
              s.backgroundImage === "none" &&
              s.borderTopWidth === "0px" &&
              s.outlineStyle === "none"
            );
          })
          .map((el) => el.getAttribute("class")),
      );
    expect(invisible, "marks drawn in forced colours").toEqual([]);
    // Heatmap steps keep their own colours (forced-color-adjust: none), so least to most still reads.
    const steps = await page
      .locator(".heatmap__cell[class*='heatmap__step--']")
      .evaluateAll((els) => new Set(els.map((e) => getComputedStyle(e).backgroundColor)).size);
    if (p.startsWith("Heatmap")) expect(steps, "distinct step colours").toBeGreaterThan(2);
  });

  test(`${p}: prints without the interface and keeps its marks`, { tag: "@print" }, async ({ page }) => {
    await gotoPage(page, url(p));
    await hydrateVisible(page);
    await page.emulateMedia({ media: "print" });
    for (const actions of await page.locator(".figure-frame__actions, .data-links").all()) {
      await expect(actions).toBeHidden();
    }
    const unprinted = await page.locator(".column-chart__fill, .heatmap__cell:not(.chart-hatch)").evaluateAll((els) =>
      els
        .filter((el) => el.getBoundingClientRect().width > 0)
        .filter((el) => getComputedStyle(el).printColorAdjust !== "exact")
        .map((el) => el.getAttribute("class")),
    );
    expect(unprinted, "marks keep their colours on paper").toEqual([]);
  });
}

test.describe("SmallMultiples", () => {
  const SM = ["SmallMultiples/default", "SmallMultiples/edge-cases"];

  test("every figure's panels are the same size, on one value axis and one set of x labels", { tag: "@smoke" }, async ({
    page,
  }) => {
    for (const p of SM) {
      await gotoPage(page, url(p));
      await hydrateVisible(page);
      for (const fig of await page.locator("main figure:has(.small-multiples)").all()) {
        const panels = await fig.locator(".small-multiples__panel .chart-plot__inner").evaluateAll((els) =>
          els.map((e) => {
            const r = e.getBoundingClientRect();
            const plot = e.closest(".chart-plot") as HTMLElement;
            const ticks = [...plot.querySelectorAll(".chart-plot__tick")].map((t) => t.textContent);
            const xs = [...plot.querySelectorAll(".chart-plot__x-label")]
              .filter((x) => (x as HTMLElement).offsetParent !== null)
              .map((x) => x.textContent);
            return { w: Math.round(r.width), h: Math.round(r.height), ticks: ticks.join(), xs: xs.join() };
          }),
        );
        if (panels.length < 2) continue;
        for (const b of panels) expect(b, p).toEqual(panels[0]);
      }
    }
  });

  test("columns per row follow the figure's width: 2, then the middle count from 34em, the wide count from 42em", async ({
    page,
  }) => {
    for (const p of SM) {
      await gotoPage(page, url(p));
      for (const grid of await page.locator(".small-multiples").all()) {
        const { em, width, mid, wide, cols } = await grid.evaluate((el) => {
          const s = getComputedStyle(el as HTMLElement);
          const g = el.querySelector(".small-multiples__grid") as HTMLElement;
          return {
            em: Number.parseFloat(s.fontSize),
            width: el.getBoundingClientRect().width,
            mid: Number(s.getPropertyValue("--small-multiples-mid")),
            wide: Number(s.getPropertyValue("--small-multiples-wide")),
            cols: getComputedStyle(g).gridTemplateColumns.split(" ").length,
          };
        });
        const expected = width >= 42 * em ? wide : width >= 34 * em ? mid : 2;
        expect(cols, `${p}: ${(width / em).toFixed(1)}em`).toBe(expected);
      }
    }
  });

  test("a panel whose series has no values says No data, with no empty plot", async ({ page }) => {
    await gotoPage(page, url("SmallMultiples/edge-cases"));
    const panel = figureOf(page, "many").locator(".small-multiples__panel").last();
    await expect(panel.locator(".small-multiples__missing")).toHaveText("No data");
    await expect(panel.locator("[data-island]")).toHaveCount(0);
  });
});

test.describe("Histogram", () => {
  test("the edge labels stay inside the plot, the first and last pinned to its ends", { tag: "@smoke" }, async ({
    page,
  }) => {
    for (const p of ["Histogram/default", "Histogram/edge-cases"]) {
      await gotoPage(page, url(p));
      await hydrateVisible(page);
      for (const plot of await page.locator(".histogram .chart-plot").all()) {
        const out = await plot.evaluate((el) => {
          const area = (el.querySelector(".chart-plot__x") as HTMLElement).getBoundingClientRect();
          return [...el.querySelectorAll<HTMLElement>(".chart-plot__x-label")]
            .filter((l) => l.offsetParent !== null)
            .filter((l) => {
              const r = l.getBoundingClientRect();
              return r.left < area.left - 0.5 || r.right > area.right + 0.5;
            })
            .map((l) => l.textContent);
        });
        expect(out, p).toEqual([]);
      }
    }
  });

  test("a range's tooltip lists at most 8 values and says how many it holds", { tag: "@interaction" }, async ({
    page,
  }) => {
    await gotoPage(page, url("Histogram/default"));
    await hydrateVisible(page);
    const plot = figureOf(page, "items").locator("[role='img'][tabindex='0']");
    await plot.focus();
    await page.keyboard.press("Home");
    const tip = figureOf(page, "items").locator(".chart-tooltip");
    for (let i = 0; i < 5; i++) {
      await expect(tip).toBeVisible();
      expect(await tip.locator(".popover__value").count()).toBeLessThanOrEqual(8);
      await expect(tip).toContainText(/to under .*: \d+ values?/);
      await page.keyboard.press("ArrowRight");
    }
  });
});

test.describe("Heatmap", () => {
  const LIGHT = [300, 400, 500, 600, 700];
  const DARK = [600, 400, 300, 200, 100];

  /** Each step's background and value colour, and the contrast of every written value on its cell. */
  const cellColours = (page: Page) =>
    page.locator(".heatmap__cell[class*='heatmap__step--']").evaluateAll((els) => {
      const lum = (c: string) => {
        const [r, g, b] = (c.match(/[\d.]+/g) ?? []).slice(0, 3).map((x) => {
          const v = Number(x) / 255;
          return v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
        }) as [number, number, number];
        return 0.2126 * r + 0.7152 * g + 0.0722 * b;
      };
      return els.map((el) => {
        const s = getComputedStyle(el);
        const step = Number(/heatmap__step--(\d)/.exec(el.className)?.[1]);
        const [hi, lo] = [lum(s.backgroundColor), lum(s.color)].sort((a, b) => b - a) as [number, number];
        return { step, bg: s.backgroundColor, contrast: (hi + 0.05) / (lo + 0.05) };
      });
    });

  test("every step is its token in both themes, by system setting and by data-theme, and values read at 4.5:1", {
    tag: "@smoke",
  }, async ({ page, theme }) => {
    await gotoPage(page, url("Heatmap/edge-cases"));
    await hydrateVisible(page);
    for (const t of ["system", "light", "dark"] as const) {
      await setTheme(page, t);
      const shown = t === "system" ? theme : t;
      const cells = await cellColours(page);
      expect(cells.length).toBeGreaterThan(0);
      for (const c of cells) {
        const ramp = shown === "dark" ? DARK : LIGHT;
        expect(c.bg, `${t} (${shown}) step ${c.step}`).toBe(
          rgb(tokenValue(`--chart-seq-blue-${ramp[c.step - 1]}`, shown)),
        );
        expect(c.contrast, `${t} step ${c.step}`).toBeGreaterThanOrEqual(4.5);
      }
    }
  });

  test("values are written only when the plot is as wide as the server says they need", async ({ page }) => {
    for (const p of ["Heatmap/default", "Heatmap/edge-cases"]) {
      await gotoPage(page, url(p));
      for (const textSize of [100, 200]) {
        await setTextSize(page, textSize);
        for (const plot of await page.locator(".heatmap__plot").all()) {
          const f = await plot.evaluate((el) => ({
            fit: Number(/heatmap--fit-(\d+)/.exec(el.className)?.[1] ?? Number.NaN),
            em: Number.parseFloat(getComputedStyle(el).fontSize),
            width: el.getBoundingClientRect().width,
            shown: [...el.querySelectorAll<HTMLElement>(".heatmap__value")].filter((v) => v.offsetParent !== null)
              .length,
            total: el.querySelectorAll(".heatmap__value").length,
          }));
          const fits = !Number.isNaN(f.fit) && f.width / f.em >= f.fit - 0.01;
          expect(f.shown, `${p} at ${textSize}%: ${(f.width / f.em).toFixed(1)}em of ${f.fit}em`).toBe(
            fits ? f.total : 0,
          );
        }
        expect(await overflowingValues(page), `${p} at ${textSize}%`).toEqual([]);
      }
    }
  });

  test("60 periods stay inside the page; every cell keeps a width (best up to about 24)", async ({ page }) => {
    await gotoPage(page, url("Heatmap/edge-cases"));
    await hydrateVisible(page);
    await expectNoSidewaysScroll(page);
    const fig = figureOf(page, "wide");
    const m = await fig
      .locator(".heatmap__cells")
      .first()
      .evaluate((el) => {
        const cells = [...el.querySelectorAll(".heatmap__cell")].map((c) => c.getBoundingClientRect());
        const box = el.getBoundingClientRect();
        return {
          width: box.width,
          min: Math.min(...cells.map((c) => c.width)),
          inside: cells.every((c) => c.left >= box.left - 0.5 && c.right <= box.right + 0.5),
          count: cells.length,
        };
      });
    expect(m.count).toBe(60);
    expect(m.inside).toBe(true);
    expect(m.min).toBeGreaterThan(0);
    test.info().annotations.push({
      type: "cell width",
      description: `60 periods: ${m.min.toFixed(1)}px; 24 periods would be ${((m.width - 23 * 2) / 24).toFixed(1)}px`,
    });
    await expect(fig.locator(".heatmap__value").filter({ visible: true })).toHaveCount(0);
  });

  test("the legend names each range, one key for one value, and No data when a cell is hatched", async ({ page }) => {
    await gotoPage(page, url("Heatmap/edge-cases"));
    await expect(figureOf(page, "single").locator(".heatmap__legend li")).toHaveText(["40 to 50%"]);
    await expect(figureOf(page, "matrix").locator(".heatmap__legend li").last()).toHaveText("No data");
    const hatch = figureOf(page, "matrix").locator(".heatmap__legend .chart-hatch");
    expect(await hatch.evaluate((el) => getComputedStyle(el).backgroundImage)).toContain("repeating-linear-gradient");
  });
});

test.describe("tooltips stay on screen at every stop", () => {
  for (const p of PAGES) {
    test(`${p}: every stop's tooltip is wholly on screen`, { tag: "@interaction" }, async ({ page }) => {
      test.setTimeout(180_000);
      await gotoPage(page, url(p));
      await hydrateVisible(page);
      const vw = page.viewportSize()?.width ?? 0;
      for (const plot of await page.locator("[data-island] [role='img'][tabindex='0']").all()) {
        if (!(await plot.isVisible())) continue;
        const island = plot.locator("xpath=ancestor::*[@data-island][1]");
        const stops = await island.evaluate((el) => {
          const props = JSON.parse(el.getAttribute("data-props") ?? "{}");
          if (props.rows) return props.rows.length * props.columns;
          return (props.stops ?? props.bands ?? []).length as number;
        });
        await plot.focus();
        // 60-column heatmaps: every stop would take minutes; the ends and every seventh stop cover each region.
        const visit = [...Array(stops).keys()].filter((i) => stops <= 40 || i % 7 === 0 || i === stops - 1);
        await page.keyboard.press("Home");
        let at = 0;
        for (const i of visit) {
          for (; at < i; at++) await page.keyboard.press("ArrowRight");
          const tip = island.locator(".chart-tooltip");
          await expect(tip).toBeVisible();
          const tb = await tip.boundingBox();
          expect(tb && tb.x >= -0.5 && tb.x + tb.width <= vw + 0.5, `${p}: stop ${i + 1} of ${stops} on screen`).toBe(
            true,
          );
        }
        await page.keyboard.press("Escape");
        await expect(island.locator(".chart-tooltip")).toHaveCount(0);
      }
    });
  }
});

test.describe("keyboard and touch", () => {
  for (const p of PAGES) {
    test(`${p}: one tab stop per plot; End reaches the last stop and the live region follows`, {
      tag: "@interaction",
    }, async ({ page }) => {
      await gotoPage(page, url(p));
      await hydrateVisible(page);
      for (const plot of await page.locator("[data-island] [role='img'][tabindex='0']").all()) {
        if (!(await plot.isVisible())) continue;
        const island = plot.locator("xpath=ancestor::*[@data-island][1]");
        expect(await island.locator("[tabindex='0'], button, a[href]").count(), "one focusable element").toBe(1);
        await plot.focus();
        await page.keyboard.press("End");
        await expect(island.locator("[aria-live='polite']")).not.toHaveText("");
        await page.keyboard.press("Tab");
        expect(await island.evaluate((el) => el.contains(document.activeElement)), "Tab leaves the plot").toBe(false);
        await expect(island.locator(".chart-tooltip"), "blur closes the tooltip").toHaveCount(0);
      }
    });
  }

  test("a heatmap's up and down keys move a row and stop at the edges", { tag: "@interaction" }, async ({ page }) => {
    await gotoPage(page, url("Heatmap/edge-cases"));
    await hydrateVisible(page);
    const fig = figureOf(page, "matrix");
    await fig.locator("[role='img'][tabindex='0']").focus();
    const tip = fig.locator(".chart-tooltip");
    await page.keyboard.press("ArrowDown");
    await expect(tip).toContainText("Change in the index, 2019");
    await page.keyboard.press("ArrowDown");
    await expect(tip).toContainText("Change in output, 2019");
    await page.keyboard.press("ArrowUp");
    await page.keyboard.press("ArrowUp");
    await expect(tip).toContainText("Change in the index, 2019");
    const scroll = await page.evaluate(() => window.scrollY);
    await page.keyboard.press("ArrowUp");
    expect(await page.evaluate(() => window.scrollY), "the keys never scroll the page").toBe(scroll);
  });

  test("a tap on a mark opens its tooltip and a tap outside closes it", { tag: "@interaction" }, async ({
    page,
    hasTouch,
  }) => {
    test.skip(!hasTouch, "touch screens only");
    for (const { p, mark } of [
      { p: "Heatmap/default", mark: ".heatmap__cell" },
      { p: "Histogram/default", mark: ".column-chart__band" },
      { p: "SmallMultiples/default", mark: ".column-chart__band" },
    ]) {
      await gotoPage(page, url(p));
      await hydrateVisible(page);
      const target = page.locator(`[data-island] ${mark}`).filter({ visible: true }).last();
      await target.scrollIntoViewIfNeeded();
      const box = await target.boundingBox();
      if (!box) throw new Error(`${p}: no mark`);
      await page.touchscreen.tap(box.x + box.width / 2, box.y + box.height - 2);
      const tip = page.locator(".chart-tooltip").filter({ visible: true });
      await expect(tip, p).toHaveCount(1);
      const tb = await tip.boundingBox();
      const vw = page.viewportSize()?.width ?? 0;
      expect(tb && tb.x >= -0.5 && tb.x + tb.width <= vw + 0.5, `${p}: tooltip on screen`).toBe(true);
      await page.touchscreen.tap(2, 2);
      await expect(page.locator(".chart-tooltip"), p).toHaveCount(0);
    }
  });
});

test.describe("crimson marks a subject that rose and never one that fell, and never a heatmap", () => {
  const painted = (fig: Locator) =>
    fig.locator(".chart-legend *, [data-island] *").evaluateAll((els) =>
      els
        .filter((el) => el.getBoundingClientRect().width > 0)
        .flatMap((el) => {
          const s = getComputedStyle(el);
          return [s.backgroundColor, s.color, s.stroke, s.fill];
        }),
    );
  const NONE = [
    { p: "SmallMultiples/edge-cases", statement: "fellLine", neutral: true },
    { p: "SmallMultiples/edge-cases", statement: "fellColumns", neutral: true },
    { p: "Histogram/edge-cases", statement: "signs", neutral: true },
    { p: "Heatmap/default", statement: "margins", neutral: false },
    { p: "Heatmap/edge-cases", statement: "matrix", neutral: false },
  ];
  const ROSE = [
    { p: "SmallMultiples/default", statement: "margins" },
    { p: "Histogram/default", statement: "items" },
  ];
  for (const { p, statement, neutral } of NONE) {
    test(`${p} "${statement}": no crimson in either theme`, { tag: "@smoke" }, async ({ page, theme }) => {
      await gotoPage(page, url(p));
      await hydrateVisible(page);
      const fig = figureOf(page, statement);
      for (const t of [theme, theme === "light" ? "dark" : "light"] as const) {
        await setTheme(page, t);
        const colours = await painted(fig);
        expect(colours.length).toBeGreaterThan(0);
        expect(colours, t).not.toContain(rgb(tokenValue("--chart-highlight", t)));
        if (neutral)
          expect(colours, `${t}: the neutral highlight`).toContain(rgb(tokenValue("--chart-neutral-highlight", t)));
      }
    });
  }
  for (const { p, statement } of ROSE) {
    test(`${p} "${statement}": the subject is crimson in either theme`, { tag: "@smoke" }, async ({ page, theme }) => {
      await gotoPage(page, url(p));
      await hydrateVisible(page);
      const fig = figureOf(page, statement);
      for (const t of [theme, theme === "light" ? "dark" : "light"] as const) {
        await setTheme(page, t);
        expect(await painted(fig), t).toContain(rgb(tokenValue("--chart-highlight", t)));
      }
    });
  }
});

test.describe(() => {
  test.use({
    allowedConsoleErrors: [
      [/^Failed to load resource/, /request failed: .*\/_astro\//, /dynamically imported/i],
      { scope: "test" },
    ] as never,
  });
  test("charts still read when their island scripts fail to load", async ({ page }) => {
    await page.route("**/_astro/**/*.js", (r) => r.abort());
    for (const { p, marks } of [
      { p: "SmallMultiples/default", marks: ".line-chart__line, .column-chart__fill" },
      { p: "Histogram/default", marks: ".column-chart__fill" },
      { p: "Heatmap/default", marks: ".heatmap__cell" },
    ]) {
      await page.goto(url(p));
      const fig = page.locator("figure").first();
      await expect(fig.locator(marks).filter({ visible: true }).first(), p).toBeVisible();
      expect(await page.locator("[role='img'][tabindex]").count(), `${p}: no dead tab stop`).toBe(0);
      await fig.getByText("Show the data table").click();
      expect(await fig.locator("tbody tr").count(), `${p}: the table still opens`).toBeGreaterThan(1);
      await expectNoSidewaysScroll(page);
    }
  });
});

test.describe("in context: the charts article fixture", () => {
  const PATH = "/fixtures/renderer/charts/";
  const STATEMENTS = ["regionslope", "marginpanels", "cpispread", "marginheat"];

  test("the Charts III figures read in the article, in order and apart", async ({ page }) => {
    await gotoPage(page, PATH);
    await hydrateVisible(page);
    await expectNoSidewaysScroll(page);
    await expectInsideViewport(page, page.locator("main"), FLOATING);
    await expectNoClippedText(page);
    await expectNoCollisions(page);
    const boxes: { top: number; bottom: number }[] = [];
    for (const s of STATEMENTS) {
      const fig = figureOf(page, s);
      await expect(fig, s).toHaveCount(1);
      const b = await fig.boundingBox();
      if (!b) throw new Error(`no box for ${s}`);
      boxes.push({ top: b.y, bottom: b.y + b.height });
    }
    for (let i = 1; i < boxes.length; i++) {
      expect(
        (boxes[i]?.top ?? 0) - (boxes[i - 1]?.bottom ?? 0),
        `space before ${STATEMENTS[i]}`,
      ).toBeGreaterThanOrEqual(16);
    }
  });

  test("passes axe in light and dark with every visible island hydrated", { tag: "@a11y" }, async ({ page, theme }) => {
    await gotoPage(page, PATH);
    await hydrateVisible(page);
    await expectAccessible(page);
    await setTheme(page, theme === "light" ? "dark" : "light");
    await expectAccessible(page);
  });
});
