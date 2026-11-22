// Component tester's checks of the x-label switch moving from `@media (min-width: 600px)` to
// `@container chart-x (min-width: 40em)` (ChartFigure.css): the wide set of x labels shows only on a plot of 40em
// of its own text. Shared by ColumnChart, LineChart, StepLineChart, Histogram and Heatmap; SmallMultiples opts out.
//   XL-1  100% text: the same label set as under the old media query at every matrix width.
//   XL-2  100% text, 430-599px: the set follows the plot's em width; no x labels overlap or crowd.
//   XL-3  200% text at 768, 1440 and 1920: the phone set, no overlapping x labels.
//   XL-4  a heatmap with long row names (a narrow inner axis) still shows the wide set at 1440.
//   XL-5  SmallMultiples never switches (its panels keep the phone set at every width).
//   XL-6  scripts blocked: the server-rendered labels are the ones shown after hydration.
import { expect, gotoPage, setTextSize, test } from "../fixtures";

type Page = import("@playwright/test").Page;

const url = (p: string) => (p.startsWith("/") ? p : `/fixtures/components/${p}/`);
const SWITCHED = [
  "ColumnChart/default",
  "ColumnChart/edge-cases",
  "LineChart/default",
  "LineChart/edge-cases",
  "StepLineChart/default",
  "StepLineChart/edge-cases",
  "Histogram/default",
  "Histogram/edge-cases",
  "Heatmap/default",
  "Heatmap/edge-cases",
  "/fixtures/renderer/charts/",
];

interface PlotLabels {
  title: string;
  em: number;
  /** "wide" or "phone" when the plot has labels that switch, else null. */
  set: "wide" | "phone" | null;
  labels: string[];
  overlaps: string[];
  /** Smallest gap in px between neighbouring x labels on one line. */
  minGap: number;
}

const settle = (page: Page) =>
  page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));

async function hydrateAll(page: Page): Promise<void> {
  const islands = page.locator("[data-island]:visible");
  for (let i = 0; i < (await islands.count()); i++) {
    await islands.nth(i).scrollIntoViewIfNeeded();
    await expect(islands.nth(i).locator("[data-hydrated='true']")).toHaveCount(1);
  }
}

/** Every outermost chart plot (a heatmap's axis is a plot inside a plot) and the x labels it shows. */
const plotLabels = (page: Page): Promise<PlotLabels[]> =>
  page.locator(".chart-plot").evaluateAll((plots) =>
    plots
      .filter((plot) => !plot.parentElement?.closest(".chart-plot"))
      .map((plot) => {
        const shown = (el: Element) => getComputedStyle(el).display !== "none" && el.getBoundingClientRect().width > 1;
        const box = plot.getBoundingClientRect();
        const wide = [...plot.querySelectorAll(".chart-plot__x-label.is-wide-only")];
        const phone = [...plot.querySelectorAll(".chart-plot__x-label.is-phone-only")];
        const set =
          wide.length + phone.length === 0
            ? null
            : (wide.length ? wide.some(shown) : !phone.some(shown))
              ? ("wide" as const)
              : ("phone" as const);
        const rects = [...plot.querySelectorAll(".chart-plot__x-label")]
          .filter(shown)
          .map((el) => ({ t: el.textContent?.trim() ?? "", r: el.getBoundingClientRect() }))
          .sort((a, b) => a.r.left - b.r.left);
        const overlaps: string[] = [];
        let minGap = Number.POSITIVE_INFINITY;
        for (let i = 0; i < rects.length; i++) {
          for (let j = i + 1; j < rects.length; j++) {
            const a = rects[i] as (typeof rects)[number];
            const b = rects[j] as (typeof rects)[number];
            const w = Math.min(a.r.right, b.r.right) - Math.max(a.r.left, b.r.left);
            const h = Math.min(a.r.bottom, b.r.bottom) - Math.max(a.r.top, b.r.top);
            if (w > 0.5 && h > 0.5) overlaps.push(`"${a.t}" x "${b.t}"`);
            if (j === i + 1 && h > 0.5) minGap = Math.min(minGap, b.r.left - a.r.right);
          }
        }
        return {
          title: plot.closest("figure")?.querySelector("[id$='-title']")?.id ?? "?",
          em: box.width / Number.parseFloat(getComputedStyle(plot).fontSize),
          set,
          labels: rects.map((x) => x.t),
          overlaps,
          minGap,
        };
      }),
  );

const widthOf = (page: Page) => page.viewportSize()?.width ?? 0;

test.describe("XL-1: at 100% text the x labels are the set the old 600px media query chose", () => {
  for (const p of SWITCHED) {
    test(`${p}: phone set under 600px, wide set from 600px`, async ({ page }) => {
      await gotoPage(page, url(p));
      await hydrateAll(page);
      const expected = widthOf(page) >= 600 ? "wide" : "phone";
      // Pages whose plans give phone and wide the same labels (StepLineChart/default, Histogram/default) have none.
      const plots = (await plotLabels(page)).filter((x) => x.set !== null);
      expect(plots.map((x) => `${x.title}: ${x.set}`)).toEqual(plots.map((x) => `${x.title}: ${expected}`));
    });
  }
});

/**
 * Open exception: from a 572px screen a plot reaches 40em (540px) and shows the wide set, 12px narrower than the
 * narrowest wide plot before (552px at 600). The edge fixture's 60 monthly columns ("Jan 2020 ... Dec 2024") then
 * have 2.9px between two labels, against 4.9px at 600 before. No overlap; listed so it cannot get worse.
 */
const CROWDED: Record<string, number> = { "ColumnChart/edge-cases long-title": 2.5 };
const GAP_FLOOR = 4; // --space-2

test.describe("XL-2: 430-599px at 100% text, where the switch moved", () => {
  for (const p of SWITCHED) {
    test(`${p}: the set follows the plot's em width; x labels never overlap or crowd`, async ({ page }) => {
      test.skip(widthOf(page) !== 430, "sweeps its own widths from the 430 project");
      await gotoPage(page, url(p));
      await hydrateAll(page);
      const problems: string[] = [];
      for (const w of [430, 500, 560, 571, 572, 580, 590, 599]) {
        await page.setViewportSize({ width: w, height: 900 });
        await settle(page);
        for (const plot of await plotLabels(page)) {
          const at = `${w}px ${plot.title} (${plot.em.toFixed(1)}em)`;
          if (plot.set !== null && plot.set !== (plot.em >= 40 ? "wide" : "phone")) problems.push(`${at}: ${plot.set}`);
          for (const o of plot.overlaps) problems.push(`${at}: overlap ${o}`);
          const floor = CROWDED[`${p} ${plot.title}`] ?? GAP_FLOOR;
          if (plot.minGap < floor) problems.push(`${at}: gap ${plot.minGap.toFixed(1)}px < ${floor}`);
        }
      }
      expect(problems).toEqual([]);
    });
  }
});

test.describe("XL-3: at 200% text a wide screen keeps the phone set", () => {
  for (const p of SWITCHED) {
    test(`${p} at 200% text: phone set, no overlapping x labels`, { tag: "@zoom" }, async ({ page }) => {
      test.skip(![768, 1440, 1920].includes(widthOf(page)), "768, 1440 and 1920 only");
      await gotoPage(page, url(p));
      await hydrateAll(page);
      await setTextSize(page, 200);
      await settle(page);
      const plots = await plotLabels(page);
      expect(
        plots.flatMap((x) => [
          ...(x.set === "wide" ? [`${x.title} (${x.em.toFixed(1)}em): wide set`] : []),
          ...x.overlaps.map((o) => `${x.title}: overlap ${o}`),
        ]),
      ).toEqual([]);
    });
  }
});

test("XL-4: a heatmap with long row names keeps the wide set at 1440 (chart-x is the whole plot)", async ({ page }) => {
  test.skip(widthOf(page) !== 1440, "1440 only");
  await gotoPage(page, url("Heatmap/edge-cases"));
  await hydrateAll(page);
  const fig = page.locator("figure:has([id='matrix-title'])");
  // Row names as long as 34 characters: the axis (an inner chart-plot after the names) narrows below 40em.
  await fig
    .locator(".chart-plot")
    .first()
    .evaluate((el) => (el as HTMLElement).style.setProperty("--chart-axis-width", "34ch"));
  await settle(page);
  const axisEm = await fig
    .locator(".heatmap__axis")
    .evaluate((el) => el.getBoundingClientRect().width / Number.parseFloat(getComputedStyle(el).fontSize));
  expect(axisEm, "inner axis narrower than 40em").toBeLessThan(40);
  const plot = (await plotLabels(page)).find((x) => x.title === "matrix-title");
  expect(plot?.em).toBeGreaterThanOrEqual(40);
  expect(plot?.set).toBe("wide");
  expect(plot?.labels).toEqual(["2019", "2020", "2021", "2022", "2023", "2024"]);
});

test.describe("XL-5: SmallMultiples does not switch", () => {
  for (const p of ["SmallMultiples/default", "SmallMultiples/edge-cases"]) {
    test(`${p}: no switched x labels, same labels at 100% and 200% text`, async ({ page }) => {
      await gotoPage(page, url(p));
      await hydrateAll(page);
      expect(await page.locator(".chart-plot__x-label.is-wide-only, .chart-plot__x-label.is-phone-only").count()).toBe(
        0,
      );
      const at100 = (await plotLabels(page)).map((x) => x.labels);
      await setTextSize(page, 200);
      await settle(page);
      const plots = await plotLabels(page);
      // A tight panel (under 20em) has its own set; every other panel keeps its labels.
      const tight = await page
        .locator(".small-multiples .chart-plot")
        .evaluateAll((els) =>
          els.map((el) => el.getBoundingClientRect().width / Number.parseFloat(getComputedStyle(el).fontSize) <= 20),
        );
      plots.forEach((x, i) => {
        if (!tight[i]) expect(x.labels, x.title).toEqual(at100[i]);
      });
    });
  }
});

test.describe("XL-6: scripts blocked, the server-rendered labels match the hydrated ones", () => {
  for (const p of SWITCHED) {
    test(`${p}: same visible x labels with and without scripts`, async ({ page, browser, browserName }) => {
      test.skip(![360, 1440].includes(widthOf(page)), "360 and 1440 only");
      // Firefox's page.evaluate hangs in a context with scripts disabled; the markup is the same in every engine.
      test.skip(browserName !== "chromium", "Chromium only");
      await gotoPage(page, url(p));
      await hydrateAll(page);
      const hydrated = (await plotLabels(page)).map((x) => `${x.title}: ${x.labels.join("|")}`);
      const noJs = await browser.newContext({ javaScriptEnabled: false, viewport: page.viewportSize() });
      const plain = await noJs.newPage();
      await gotoPage(plain, url(p));
      const server = (await plotLabels(plain)).map((x) => `${x.title}: ${x.labels.join("|")}`);
      await noJs.close();
      expect(server).toEqual(hydrated);
    });
  }
});
