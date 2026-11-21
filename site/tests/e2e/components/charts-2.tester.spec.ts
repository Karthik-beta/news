// Component tester's browser checks for Charts II (#13, part 1): Waterfall, Sankey, StackedBar, SegmentedBar,
// DotPlot (dots and dumbbells) and Slope, on top of the builder's charts-2.spec.ts. Every fixture page on every
// screen: nothing clipped, off screen or overlapping (labels against labels, Sankey labels against other nodes),
// 200% text and 200% zoom, no motion and no shift, forced colours, print, island scripts blocked; where the Sankey
// switches to its waterfall (by the plot's width in its own em); no flow under 2px on screen; a pointer on the
// thinnest flow shows its node; every tooltip stop stays on screen; one tab stop per plot with every stop reachable;
// taps; crimson never marks a subject that fell, and marks one that rose; and the charts in context on the charts
// article fixture.
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

const NAMES = ["Waterfall", "Sankey", "StackedBar", "SegmentedBar", "DotPlot", "Slope"];
const PAGES = NAMES.flatMap((n) => [`${n}/default`, `${n}/edge-cases`]);
const url = (p: string) => `/fixtures/components/${p}/`;
/** Chart tooltips and popover panels place themselves; their own tests check them. */
const FLOATING = "[popover], .popover--tooltip, .chart-tooltip";

/** Every label a plot writes: none may overlap another. */
const LABELS = [
  ".sankey__label",
  ".slope__label",
  ".slope__period",
  ".dot-plot__label",
  ".dot-plot__value",
  ".stacked-bar__label",
  ".stacked-bar__total",
  ".bar-chart__label",
  ".bar-chart__value",
  ".segmented-bar__item",
].join(", ");

const figureOf = (page: Page, statement: string) => page.locator(`figure:has([id="${statement}-title"])`);

/** Hydrates every island the reader can see (a Sankey's hidden half never hydrates, by design). */
async function hydrateVisible(page: Page): Promise<void> {
  const islands = page.locator("[data-island]");
  for (let i = 0; i < (await islands.count()); i++) {
    const island = islands.nth(i);
    if (!(await island.isVisible())) continue;
    await island.scrollIntoViewIfNeeded();
    await expect(island.locator("[data-hydrated='true']")).toHaveCount(1);
  }
}

/**
 * Pairs of visible plot labels whose text overlaps (tight text boxes from Range rects, so a label's empty
 * max-content box does not count), and Sankey labels that cover a node other than their own.
 */
async function collisions(scope: Locator): Promise<string[]> {
  return scope.evaluate((root, selector) => {
    const textBoxes = (el: Element) => {
      const range = document.createRange();
      const out: DOMRect[] = [];
      const walk = document.createTreeWalker(el, NodeFilter.SHOW_TEXT);
      for (let n = walk.nextNode(); n; n = walk.nextNode()) {
        if (!n.textContent?.trim()) continue;
        range.selectNodeContents(n);
        for (const r of range.getClientRects()) if (r.width > 0 && r.height > 0) out.push(r);
      }
      return out;
    };
    const visible = (el: Element) => {
      const s = getComputedStyle(el);
      return s.visibility !== "hidden" && (el as HTMLElement).offsetParent !== null;
    };
    const overlap = (a: DOMRect, b: DOMRect) =>
      Math.min(a.right, b.right) - Math.max(a.left, b.left) > 1 &&
      Math.min(a.bottom, b.bottom) - Math.max(a.top, b.top) > 1;
    const name = (el: Element) => `${el.className.toString().split(" ")[0]} "${el.textContent?.trim().slice(0, 30)}"`;
    const labels = [...root.querySelectorAll(selector)].filter(visible);
    const boxes = labels.map((el) => ({ el, rects: textBoxes(el) }));
    const found: string[] = [];
    for (let i = 0; i < boxes.length; i++) {
      for (let j = i + 1; j < boxes.length; j++) {
        const a = boxes[i] as (typeof boxes)[number];
        const b = boxes[j] as (typeof boxes)[number];
        if (a.el.contains(b.el) || b.el.contains(a.el)) continue;
        if (a.rects.some((ra) => b.rects.some((rb) => overlap(ra, rb)))) found.push(`${name(a.el)} × ${name(b.el)}`);
      }
    }
    // A Sankey label may sit over flows (on its paper ground) but never over another node.
    for (const area of root.querySelectorAll(".sankey__area")) {
      if (!visible(area)) continue;
      const nodes = [...area.querySelectorAll<HTMLElement>(".sankey__node")];
      for (const label of area.querySelectorAll<HTMLElement>(".sankey__label")) {
        const own = label.dataset.node;
        for (const node of nodes) {
          if (node.dataset.node === own) continue;
          const nr = node.getBoundingClientRect();
          if (textBoxes(label).some((r) => overlap(r, nr))) found.push(`${name(label)} over node ${node.dataset.node}`);
        }
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

for (const p of PAGES) {
  test(`${p}: nothing clipped, off screen or overlapping once hydrated`, async ({ page }) => {
    await gotoPage(page, url(p));
    await hydrateVisible(page);
    await expectNoSidewaysScroll(page);
    await expectInsideViewport(page, page.locator("main"), FLOATING);
    await expectNoClippedText(page);
    await expectNoCollisions(page);
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
    await setTextSize(page, 100);
    test.skip(!(await zoomTo200(page)), "below 640px wide, 200% zoom is the 320 project (400%)");
    await hydrateVisible(page);
    await expectNoSidewaysScroll(page);
    await expectInsideViewport(page, page.locator("main"), FLOATING);
    await expectNoCollisions(page);
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
      .locator(
        ".bar-chart__bar, .stacked-bar__segment, .dot-plot__dot, .slope__dot, .sankey__node, .chart-key, .slope__line, .sankey__flow",
      )
      .evaluateAll((els) =>
        els
          .filter((el) => {
            const r = el.getBoundingClientRect();
            if (r.width === 0 && r.height === 0) return false; // hidden half of a Sankey
            const s = getComputedStyle(el);
            const svg = el instanceof SVGElement;
            const canvas = getComputedStyle(document.body).backgroundColor;
            if (svg) {
              const paint = el.tagName === "line" ? s.stroke : s.fill;
              return paint === "none" || paint === "rgba(0, 0, 0, 0)" || paint === canvas;
            }
            return (
              (s.backgroundColor === "rgba(0, 0, 0, 0)" || s.backgroundColor === canvas) &&
              s.backgroundImage === "none" &&
              s.borderTopWidth === "0px"
            );
          })
          .map((el) => el.getAttribute("class")),
      );
    expect(invisible, "marks drawn in forced colours").toEqual([]);
  });

  test(`${p}: prints without the interface and keeps its marks`, { tag: "@print" }, async ({ page }) => {
    await gotoPage(page, url(p));
    await hydrateVisible(page);
    await page.emulateMedia({ media: "print" });
    for (const actions of await page.locator(".figure-frame__actions, .data-links").all()) {
      await expect(actions).toBeHidden();
    }
    const unprinted = await page
      .locator(".bar-chart__bar, .stacked-bar__segment, .dot-plot__dot, .slope__dot, .sankey__node")
      .evaluateAll((els) =>
        els
          .filter((el) => el.getBoundingClientRect().width > 0)
          .filter((el) => getComputedStyle(el).printColorAdjust !== "exact")
          .map((el) => el.getAttribute("class")),
      );
    expect(unprinted, "marks keep their colours on paper").toEqual([]);
    for (const fig of await page.locator("figure").all()) await expect(fig.locator("figcaption")).toBeVisible();
  });
}

test.describe("the Sankey switches to its waterfall by its plot's width", () => {
  /** The width a Sankey of `columns` columns needs, in the plot's em (design/charts.md, "When it shows"). */
  const needEm = (columns: number) => 14 + 7.5 * (columns - 1);

  for (const textSize of [100, 200]) {
    test(`at ${textSize}% text: flows only when the plot is at least as wide as its columns need`, {
      tag: textSize === 100 ? "@smoke" : "@zoom",
    }, async ({ page }) => {
      for (const p of ["Sankey/default", "Sankey/edge-cases"]) {
        await gotoPage(page, url(p));
        await setTextSize(page, textSize);
        for (const sankey of await page.locator(".sankey").all()) {
          const { em, width, columns, fallbackOnly } = await sankey.evaluate((el) => ({
            em: Number.parseFloat(getComputedStyle(el).fontSize),
            width: el.getBoundingClientRect().width,
            columns: Number(/sankey--c(\d)/.exec(el.className)?.[1] ?? 0),
            fallbackOnly: el.classList.contains("sankey--fallback-only"),
          }));
          const flows = sankey.locator(".sankey__flows");
          const fallback = sankey.locator(".sankey__fallback");
          if (fallbackOnly) {
            await expect(fallback).toBeVisible();
            await expect(flows).toHaveCount(0);
            continue;
          }
          const fits = width / em >= needEm(columns) - 0.01;
          test.info().annotations.push({
            type: `${p} ${columns} columns`,
            description: `${(width / em).toFixed(1)}em of ${needEm(columns)}em → ${fits ? "flows" : "waterfall"}`,
          });
          await expect(fits ? flows : fallback, `${p}: ${(width / em).toFixed(1)}em`).toBeVisible();
          await expect(fits ? fallback : flows).toBeHidden();
          // Issue #13: at 360 every Sankey shows its waterfall; a full statement (5 columns) does on every phone.
          // A 3-column Sankey needs only 29em (about 392px), so a 430 phone shows its flows (question O-1).
          const vw = page.viewportSize()?.width ?? 0;
          if (vw <= 360 || (vw <= 430 && columns >= 5)) expect(fits, "this phone gets the waterfall").toBe(false);
        }
      }
    });
  }
});

test.describe("Sankey flows", () => {
  test("no flow is drawn thinner than 2px on screen", { tag: "@smoke" }, async ({ page }) => {
    for (const p of ["Sankey/default", "Sankey/edge-cases"]) {
      await gotoPage(page, url(p));
      for (const island of await page.locator(".sankey__flows [data-island]").all()) {
        if (!(await island.isVisible())) continue;
        const thinnest = await island.evaluate((el) => {
          const props = JSON.parse(el.getAttribute("data-props") ?? "{}") as {
            height: number;
            flows: { d: string }[];
          };
          const svg = el.querySelector("svg") as SVGSVGElement;
          const scale = svg.getBoundingClientRect().height / props.height;
          return Math.min(
            ...props.flows.map((f) => {
              const n = (f.d.match(/-?\d+(\.\d+)?/g) ?? []).map(Number);
              // M x0 t0 C xm t0 xm t1 x1 t1 L x1 b1 C xm b1 xm b0 x0 b0 Z
              return ((n[15] as number) - (n[1] as number)) * scale;
            }),
          );
        });
        expect(thinnest, p).toBeGreaterThanOrEqual(2 - 0.01);
      }
    }
  });

  test("a pointer on the thinnest flow shows the node it belongs to", { tag: "@interaction" }, async ({
    page,
    hasTouch,
  }) => {
    test.skip(hasTouch, "hover needs a mouse; taps are checked separately");
    await gotoPage(page, url("Sankey/default"));
    const island = page.locator(".sankey__flows [data-island]").first();
    test.skip(!(await island.isVisible()), "the waterfall shows at this width");
    await island.scrollIntoViewIfNeeded();
    await expect(island.locator("[data-hydrated='true']")).toHaveCount(1);
    const target = await island.evaluate((el) => {
      const props = JSON.parse(el.getAttribute("data-props") ?? "{}") as {
        height: number;
        flows: { d: string; owner: number }[];
        nodes: { label: string }[];
      };
      const box = (el.querySelector("svg") as SVGSVGElement).getBoundingClientRect();
      const flows = props.flows.map((f) => {
        const n = (f.d.match(/-?\d+(\.\d+)?/g) ?? []).map(Number);
        const [x0, t0, , , , , x1, t1] = n as number[];
        const thickness = (n[15] as number) - (t0 as number);
        return {
          owner: f.owner,
          thickness,
          x: ((x0 as number) + (x1 as number)) / 2,
          y: ((t0 as number) + (t1 as number)) / 2 + thickness / 2,
        };
      });
      const thin = flows.sort((a, b) => a.thickness - b.thickness)[0] as (typeof flows)[number];
      return {
        x: box.left + (thin.x / 1000) * box.width,
        y: box.top + (thin.y / props.height) * box.height,
        label: props.nodes[thin.owner]?.label ?? "",
        px: (thin.thickness * box.height) / props.height,
      };
    });
    test.info().annotations.push({ type: "thinnest flow", description: `${target.label}, ${target.px.toFixed(1)}px` });
    await page.mouse.move(target.x, target.y);
    await expect(island.locator(".chart-tooltip")).toContainText(target.label);
  });
});

test.describe("tooltips stay on screen at every stop", () => {
  for (const p of PAGES) {
    test(`${p}: every stop's tooltip is wholly on screen`, { tag: "@interaction" }, async ({ page }) => {
      test.setTimeout(120_000);
      await gotoPage(page, url(p));
      await hydrateVisible(page);
      const vw = page.viewportSize()?.width ?? 0;
      for (const plot of await page.locator("[data-island] [role='img'][tabindex='0']").all()) {
        if (!(await plot.isVisible())) continue;
        const island = plot.locator("xpath=ancestor::*[@data-island][1]");
        const stops = await island.evaluate((el) => {
          const props = JSON.parse(el.getAttribute("data-props") ?? "{}");
          return (props.stops ?? props.rows ?? props.nodes ?? props.items ?? props.bands ?? []).length as number;
        });
        await plot.focus();
        for (let i = 0; i < stops; i++) {
          await page.keyboard.press(i === 0 ? "Home" : "ArrowRight");
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
      const plots = page.locator("[data-island] [role='img'][tabindex='0']");
      const visible: Locator[] = [];
      for (const plot of await plots.all()) if (await plot.isVisible()) visible.push(plot);
      for (const plot of visible) {
        const island = plot.locator("xpath=ancestor::*[@data-island][1]");
        expect(await island.locator("[tabindex='0'], button, a[href]").count(), "one focusable element").toBe(1);
        await plot.focus();
        await page.keyboard.press("End");
        const live = island.locator("[aria-live='polite']");
        await expect(live).not.toHaveText("");
        await page.keyboard.press("Tab");
        expect(await island.evaluate((el) => el.contains(document.activeElement)), "Tab leaves the plot").toBe(false);
        await expect(island.locator(".chart-tooltip"), "blur closes the tooltip").toHaveCount(0);
      }
    });
  }

  test("a tap on a mark opens its tooltip and a tap outside closes it", { tag: "@interaction" }, async ({
    page,
    hasTouch,
  }) => {
    test.skip(!hasTouch, "touch screens only");
    for (const { p, mark } of [
      // On phones and small tablets the Sankey shows its waterfall: tap whichever the reader sees.
      { p: "Sankey/default", mark: ".sankey__node, .bar-chart__bar" },
      { p: "Waterfall/default", mark: ".bar-chart__bar" },
      { p: "StackedBar/default", mark: ".stacked-bar__segment" },
      { p: "SegmentedBar/default", mark: ".stacked-bar__segment" },
      { p: "DotPlot/default", mark: ".dot-plot__dot" },
      { p: "Slope/default", mark: ".slope__dot" },
    ]) {
      await gotoPage(page, url(p));
      await hydrateVisible(page);
      const target = page.locator(`[data-island] ${mark}`).filter({ visible: true }).last();
      await target.scrollIntoViewIfNeeded();
      const box = await target.boundingBox();
      if (!box) throw new Error(`${p}: no mark`);
      await page.touchscreen.tap(box.x + box.width / 2, box.y + box.height / 2);
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

test.describe("crimson marks a subject that rose and never one that fell (Karthik, 16 Nov 2026)", () => {
  const FELL = [
    { p: "Waterfall/edge-cases", statement: "loss" },
    { p: "Waterfall/edge-cases", statement: "mismatch" },
    { p: "Sankey/edge-cases", statement: "loss" },
    { p: "StackedBar/edge-cases", statement: "fell" },
    { p: "SegmentedBar/edge-cases", statement: "fell" },
    { p: "DotPlot/edge-cases", statement: "fell" },
    { p: "DotPlot/edge-cases", statement: "signs" },
    { p: "Slope/edge-cases", statement: "fell" },
  ];
  const ROSE = [
    { p: "Waterfall/default", statement: "profit" },
    { p: "Sankey/default", statement: "flows" },
    { p: "StackedBar/default", statement: "europe" },
    { p: "SegmentedBar/default", statement: "mix" },
    { p: "DotPlot/default", statement: "dumbbell" },
    { p: "DotPlot/default", statement: "dots" },
    { p: "Slope/default", statement: "europe" },
  ];
  /** Every colour painted inside a figure's plot, legend and key (marks, labels, keys, flows). */
  const painted = (fig: Locator) =>
    fig.locator(".chart-legend *, [data-island] *, .segmented-bar__key *").evaluateAll((els) =>
      els
        .filter((el) => el.getBoundingClientRect().width > 0)
        .flatMap((el) => {
          const s = getComputedStyle(el);
          return [s.backgroundColor, s.color, s.stroke, s.fill];
        }),
    );

  for (const { p, statement } of FELL) {
    test(`${p} "${statement}": no crimson anywhere, in either theme`, { tag: "@smoke" }, async ({ page, theme }) => {
      await gotoPage(page, url(p));
      await hydrateVisible(page);
      const fig = figureOf(page, statement);
      for (const t of [theme, theme === "light" ? "dark" : "light"] as const) {
        await setTheme(page, t);
        const colours = await painted(fig);
        expect(colours.length, "marks found").toBeGreaterThan(0);
        expect(colours, `${t}: the fallen subject is not crimson`).not.toContain(
          rgb(tokenValue("--chart-highlight", t)),
        );
        expect(colours, `${t}: it takes the neutral highlight`).toContain(
          rgb(tokenValue("--chart-neutral-highlight", t)),
        );
      }
    });
  }

  for (const { p, statement } of ROSE) {
    test(`${p} "${statement}": the subject that rose is crimson, in either theme`, { tag: "@smoke" }, async ({
      page,
      theme,
    }) => {
      await gotoPage(page, url(p));
      await hydrateVisible(page);
      const fig = figureOf(page, statement);
      for (const t of [theme, theme === "light" ? "dark" : "light"] as const) {
        await setTheme(page, t);
        const colours = await painted(fig);
        expect(colours, t).toContain(rgb(tokenValue("--chart-highlight", t)));
        expect(colours, t).not.toContain(rgb(tokenValue("--chart-neutral-highlight", t)));
      }
    });
  }
});

test("a mark with no data is hatched, never an empty box (C2-5)", async ({ page }) => {
  for (const p of ["StackedBar/edge-cases", "Waterfall/edge-cases"]) {
    await gotoPage(page, url(p));
    await hydrateVisible(page);
    const hatches = page.locator("[data-island] .chart-hatch");
    expect(await hatches.count(), p).toBeGreaterThan(0);
    for (const h of await hatches.all()) {
      expect(await h.evaluate((el) => getComputedStyle(el).backgroundImage), p).toContain("repeating-linear-gradient");
    }
  }
});

test("forced colours: a highlighted part that fell is drawn in Highlight like any subject (C2-6)", {
  tag: "@forced-colors",
}, async ({ page }) => {
  await gotoPage(page, url("StackedBar/edge-cases"));
  await hydrateVisible(page);
  const fig = figureOf(page, "fell");
  const subject = fig.locator(".stacked-bar__segment.chart-colour--highlight-neutral").first();
  const other = fig.locator(".stacked-bar__segment.chart-colour--context").first();
  const [s, o] = await Promise.all(
    [subject, other].map((l) => l.evaluate((el) => getComputedStyle(el).backgroundColor)),
  );
  expect(s, "the subject stands out").not.toBe(o);
});

test.describe(() => {
  // Blocking the island scripts is the failure under test.
  test.use({
    allowedConsoleErrors: [
      [/^Failed to load resource/, /request failed: .*\/_astro\//, /dynamically imported/i],
      { scope: "test" },
    ] as never,
  });
  test("charts still read when their island scripts fail to load", async ({ page }) => {
    await page.route("**/_astro/**/*.js", (r) => r.abort());
    for (const { p, marks, values } of [
      { p: "Waterfall/default", marks: ".bar-chart__bar", values: ".bar-chart__value" },
      { p: "Sankey/default", marks: ".sankey__node, .bar-chart__bar", values: ".sankey__value, .bar-chart__value" },
      { p: "StackedBar/default", marks: ".stacked-bar__segment", values: ".stacked-bar__total" },
      { p: "SegmentedBar/default", marks: ".stacked-bar__segment", values: ".segmented-bar__value" },
      { p: "DotPlot/default", marks: ".dot-plot__dot", values: ".dot-plot__value" },
      { p: "Slope/default", marks: ".slope__dot", values: ".slope__value" },
    ]) {
      await page.goto(url(p));
      const fig = page.locator("figure").first();
      await expect(fig.locator(marks).filter({ visible: true }).first(), p).toBeVisible();
      await expect(fig.locator(values).filter({ visible: true }).first(), p).toBeVisible();
      expect(await page.locator("[role='img'][tabindex]").count(), `${p}: no dead tab stop`).toBe(0);
      await fig.getByText("Show the data table").click();
      expect(await fig.locator("tbody tr").count(), `${p}: the table still opens`).toBeGreaterThan(1);
      await expectNoSidewaysScroll(page);
    }
  });
});

test.describe("in context: the charts article fixture", () => {
  const PATH = "/fixtures/renderer/charts/";
  const STATEMENTS = ["income", "flows", "regionmix", "categorymix", "regionchange", "regionslope"];

  test("the Charts II figures read in the article, in order and apart", async ({ page }) => {
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
