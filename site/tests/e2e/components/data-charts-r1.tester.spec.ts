// Component tester's re-test of #8's fix round in the browser (the rules themselves are in
// tests/unit/data-charts-r1.tester.test.ts):
//   DC-1  a highlighted subject that fell is painted in chart.neutral-highlight, never crimson: the bar, the column,
//         the line and its end marker, the step line, the bold labels, the legend key and the data table's row bar,
//         in light and dark (system setting and data-theme), and Highlight in forced colours like the crimson one.
//   DC-2  column, line and step-line plots at 100% and 200% text: no sideways page scroll, every label inside its
//         plot, no two labels overlapping, no clipped text; the tight layout only when the plot is under 20em of its
//         own text, so at 100% text nothing changes.
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { siteRoot, tokenValue } from "../../support/tokens";
import { expect, expectNoSidewaysScroll, gotoPage, setTextSize, setTheme, type Theme, test } from "../fixtures";
import { expectNoClippedText, rgb } from "./primitives";

type Page = import("@playwright/test").Page;
type Locator = import("@playwright/test").Locator;

const url = (p: string) => `/fixtures/components/${p}/`;
const figureOf = (page: Page, statement: string) => page.locator(`figure:has([id="${statement}-title"])`);

async function hydrateAll(page: Page): Promise<void> {
  const islands = page.locator("[data-island]");
  for (let i = 0; i < (await islands.count()); i++) {
    await islands.nth(i).scrollIntoViewIfNeeded();
    await expect(islands.nth(i).locator("[data-hydrated='true']")).toHaveCount(1);
  }
}

/** Every colour an element paints with: its background, text, stroke and fill. */
const paints = (loc: Locator) =>
  loc.evaluateAll((els) =>
    els.flatMap((el) => {
      const s = getComputedStyle(el);
      return [s.backgroundColor, s.color, s.stroke, s.fill].filter((c) => c && c !== "none");
    }),
  );

test.describe("DC-1: a highlighted subject that fell is painted in the neutral highlight", () => {
  // The subject's marks and labels on each fixture; `paint` is the property that carries the mark's colour.
  const CASES = [
    {
      p: "BarChart/edge-cases",
      statement: "fell",
      marks: [
        { sel: ".bar-chart__bar.chart-colour--highlight-neutral", paint: "backgroundColor" },
        { sel: ".bar-chart__value.is-highlight-neutral", paint: "color" },
      ],
    },
    {
      p: "ColumnChart/edge-cases",
      statement: "fell",
      marks: [
        { sel: ".column-chart__fill.chart-colour--highlight-neutral", paint: "backgroundColor" },
        { sel: ".column-chart__value.is-highlight-neutral", paint: "color" },
      ],
    },
    {
      p: "LineChart/edge-cases",
      statement: "fell",
      marks: [
        { sel: ".line-chart__line.chart-colour--highlight-neutral", paint: "stroke" },
        { sel: ".line-chart__marker.chart-colour--highlight-neutral", paint: "backgroundColor" },
      ],
    },
    {
      // Three lines, the subject with a gap, 1.5% to 0.9%: the legend key and the bold end name follow it.
      p: "LineChart/edge-cases",
      statement: "highlightContext",
      marks: [
        { sel: ".line-chart__line.chart-colour--highlight-neutral", paint: "stroke" },
        { sel: ".chart-key.chart-colour--highlight-neutral", paint: "backgroundColor" },
        { sel: ".line-chart__end-label.is-highlight-neutral .line-chart__end-name", paint: "color" },
      ],
    },
    {
      p: "StepLineChart/edge-cases",
      statement: "fell",
      marks: [
        { sel: ".line-chart__line.chart-colour--highlight-neutral", paint: "stroke" },
        { sel: ".line-chart__marker.chart-colour--highlight-neutral", paint: "backgroundColor" },
      ],
    },
  ] as const;

  for (const { p, statement, marks } of CASES) {
    test(`${p} "${statement}": neutral highlight in light and dark, never crimson`, { tag: "@smoke" }, async ({
      page,
      theme,
    }) => {
      await gotoPage(page, url(p));
      await hydrateAll(page);
      const fig = figureOf(page, statement);
      for (const t of ["system", theme === "light" ? "dark" : "light"] as const) {
        await setTheme(page, t);
        const shown: Theme = t === "system" ? theme : t;
        const neutral = rgb(tokenValue("--chart-neutral-highlight", shown));
        const crimson = rgb(tokenValue("--chart-highlight", shown));
        for (const { sel, paint } of marks) {
          const loc = fig.locator(sel);
          expect(await loc.count(), `${shown}: ${sel} present`).toBeGreaterThan(0);
          const colours = await loc.evaluateAll(
            (els, prop) => els.map((el) => getComputedStyle(el)[prop as "color"]),
            paint,
          );
          for (const c of colours) expect(c, `${shown}: ${sel} ${paint}`).toBe(neutral);
        }
        const everything = await paints(fig.locator("*"));
        expect(everything, `${shown}: nothing in the figure is crimson`).not.toContain(crimson);
      }
    });
  }

  test("the data table's row bar is the neutral highlight for a fall and crimson for a rise", {
    tag: "@smoke",
  }, async ({ page, theme }) => {
    await gotoPage(page, url("DataTable/edge-cases"));
    const bar = (statement: string) =>
      figureOf(page, statement)
        .locator(".data-table__row.is-highlight > .data-table__row-head")
        .evaluate((el) => getComputedStyle(el, "::before").backgroundColor);
    for (const t of ["system", theme === "light" ? "dark" : "light"] as const) {
      await setTheme(page, t);
      const shown: Theme = t === "system" ? theme : t;
      expect(await bar("fell"), `${shown}: a period that fell`).toBe(
        rgb(tokenValue("--chart-neutral-highlight", shown)),
      );
      expect(await bar("fellSeries"), `${shown}: a series that fell`).toBe(
        rgb(tokenValue("--chart-neutral-highlight", shown)),
      );
    }
    await gotoPage(page, url("DataTable/default"));
    await setTheme(page, theme);
    expect(await bar("margins"), "a period that rose stays crimson").toBe(rgb(tokenValue("--chart-highlight", theme)));
  });

  test("the bar chart's own data table marks the fallen row too", async ({ page, theme }) => {
    await gotoPage(page, url("BarChart/edge-cases"));
    const fig = figureOf(page, "fell");
    // The table is in the page whether or not its toggle is open; the row bar's colour is the same either way.
    const head = fig.locator(".data-table__row.is-highlight.is-fell > .data-table__row-head");
    await expect(head).toHaveCount(1);
    expect(await head.evaluate((el) => getComputedStyle(el, "::before").backgroundColor)).toBe(
      rgb(tokenValue("--chart-neutral-highlight", theme)),
    );
  });

  test("forced colours: the fallen subject is drawn in Highlight like any highlighted subject", {
    tag: "@forced-colors",
  }, async ({ page }) => {
    const highlight = await page.evaluate(() => {
      const probe = document.createElement("span");
      probe.style.color = "Highlight";
      document.body.append(probe);
      const c = getComputedStyle(probe).color;
      probe.remove();
      return c;
    });
    await gotoPage(page, url("DataTable/edge-cases"));
    for (const statement of ["fell", "fellSeries"]) {
      const bar = await figureOf(page, statement)
        .locator(".data-table__row.is-highlight > .data-table__row-head")
        .evaluate((el) => getComputedStyle(el, "::before").backgroundColor);
      expect(bar, `DataTable "${statement}" row bar`).toBe(highlight);
    }
    await gotoPage(page, url("BarChart/edge-cases"));
    await hydrateAll(page);
    expect(
      await figureOf(page, "fell")
        .locator(".bar-chart__bar.chart-colour--highlight-neutral")
        .evaluate((el) => getComputedStyle(el).backgroundColor),
    ).toBe(highlight);
    await gotoPage(page, url("LineChart/edge-cases"));
    await hydrateAll(page);
    expect(
      await figureOf(page, "fell")
        .locator(".line-chart__line.chart-colour--highlight-neutral")
        .evaluate((el) => getComputedStyle(el).stroke),
    ).toBe(highlight);
  });
});

/**
 * Open exceptions, in data-charts-r1.known.json, keyed "<viewport width> <text size>%"; anything not listed fails.
 * All but one were there before the fix round: measured with this same audit on the pre-fix build (6018247, the
 * merge before the fix commits) and unchanged by it. They are a few labels at 100% text that the fix round was told
 * to leave as they were (a last x label or end label a few px past the plot's edge at 320-430; a negative column's
 * value over the first x label at 768 and 1440; x labels of the 1e17 and 1e-8 edge fixtures touching at 320), the
 * same negative value and two neighbouring 1e17 values touching at 768 and 1440 with 200% text, and one at 320 and
 * 200% text: the 1e-8 edge fixture's latest value (eleven digits) is wider than the 114px left for its columns and
 * crosses the axis tick. The sign-offs list them.
 */
const KNOWN: Record<string, string[]> = JSON.parse(
  readFileSync(join(siteRoot, "tests/e2e/components/data-charts-r1.known.json"), "utf8"),
);

test.describe("DC-2: chart labels stay inside their plots at 100% and 200% text", () => {
  const PAGES = [
    "ColumnChart/default",
    "ColumnChart/edge-cases",
    "LineChart/default",
    "LineChart/edge-cases",
    "StepLineChart/default",
    "StepLineChart/edge-cases",
  ];
  const LABELS = [
    ".chart-plot__x-label",
    ".chart-plot__tick",
    ".line-chart__end-label",
    ".line-chart__ends-item",
    ".line-chart__event-label",
    ".line-chart__events-label",
    ".column-chart__value",
  ].join(", ");

  /** For every plot: labels outside the plot box, pairs of overlapping labels, and whether it is tight. */
  const audit = (page: Page) =>
    page.locator(".chart-plot").evaluateAll((plots, sel) => {
      const visible = (el: Element) => {
        const s = getComputedStyle(el);
        const r = el.getBoundingClientRect();
        return (
          s.display !== "none" && s.visibility !== "hidden" && Number(s.opacity) > 0 && r.width > 1 && r.height > 1
        );
      };
      const name = (el: Element) => `${el.className.toString().split(" ")[0]} "${el.textContent?.trim().slice(0, 24)}"`;
      return plots.map((plot) => {
        const box = plot.getBoundingClientRect();
        const fig = plot.closest("figure");
        const title = fig?.querySelector("[id$='-title']")?.id ?? "?";
        const labels = [...plot.querySelectorAll(sel)].filter(visible);
        const rects = labels.map((el) => ({ el, r: el.getBoundingClientRect() }));
        // Inside the plot box: across for every label; down for every label but the value-axis ticks, which sit
        // above their gridlines by design (the top one above the plot's top edge).
        const tick = (el: Element) => el.classList.contains("chart-plot__tick");
        const outside = rects
          .filter(
            ({ el, r }) =>
              r.left < box.left - 0.5 ||
              r.right > box.right + 0.5 ||
              (!tick(el) && (r.top < box.top - 0.5 || r.bottom > box.bottom + 0.5)),
          )
          .map(({ el, r }) => ({
            key: name(el),
            at: `[${Math.round(r.left)},${Math.round(r.top)} ${Math.round(r.right)},${Math.round(r.bottom)}] in [${Math.round(box.left)},${Math.round(box.top)} ${Math.round(box.right)},${Math.round(box.bottom)}]`,
          }));
        const overlaps: string[] = [];
        for (let i = 0; i < rects.length; i++) {
          for (let j = i + 1; j < rects.length; j++) {
            const a = rects[i] as (typeof rects)[number];
            const b = rects[j] as (typeof rects)[number];
            if (a.el.contains(b.el) || b.el.contains(a.el)) continue;
            const w = Math.min(a.r.right, b.r.right) - Math.max(a.r.left, b.r.left);
            const h = Math.min(a.r.bottom, b.r.bottom) - Math.max(a.r.top, b.r.top);
            if (w > 0.5 && h > 0.5) overlaps.push(`${name(a.el)} x ${name(b.el)}`);
          }
        }
        const ends = plot.querySelector(".line-chart__ends");
        const tight = ends ? getComputedStyle(ends).display !== "none" : null;
        const fontPx = Number.parseFloat(getComputedStyle(plot).fontSize);
        return { title, width: box.width, em: box.width / fontPx, tight, outside, overlaps, count: labels.length };
      });
    }, LABELS);

  for (const p of PAGES) {
    for (const size of [100, 200]) {
      test(`${p} at ${size}% text: no sideways scroll, labels inside, none overlapping or clipped`, {
        tag: "@zoom",
      }, async ({ page }) => {
        await gotoPage(page, url(p));
        await hydrateAll(page);
        await setTextSize(page, size);
        await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
        await expectNoSidewaysScroll(page);
        await expectNoClippedText(page);
        const plots = await audit(page);
        expect(plots.length, "plots on the page").toBeGreaterThan(0);
        test.info().annotations.push({
          type: "plot widths (em)",
          description: plots.map((x) => `${x.title}=${x.em.toFixed(1)}${x.tight ? " tight" : ""}`).join(", "),
        });
        const problems = plots.flatMap((plot) => {
          const where = `${plot.title} (${Math.round(plot.width)}px, ${plot.em.toFixed(1)}em)`;
          return [
            ...(plot.count > 0 ? [] : [{ key: `${plot.title}: no labels`, text: `${where}: no labels shown` }]),
            ...plot.outside.map((o) => ({
              key: `${plot.title}: outside: ${o.key}`,
              text: `${where}: outside the plot: ${o.key} ${o.at}`,
            })),
            ...plot.overlaps.map((o) => ({ key: `${plot.title}: overlap: ${o}`, text: `${where}: overlapping: ${o}` })),
            // The tight layout is used exactly when the plot is narrower than 20em of its own text, never at 100%.
            ...(plot.tight !== null && plot.tight !== plot.em <= 20
              ? [{ key: `${plot.title}: tight`, text: `${where}: tight is ${plot.tight}` }]
              : []),
            ...(size === 100 && plot.em <= 20
              ? [{ key: `${plot.title}: tight`, text: `${where}: tight at 100%` }]
              : []),
          ];
        });
        const known = new Set(KNOWN[`${page.viewportSize()?.width} ${size}%`] ?? []);
        const open = problems.filter((x) => known.has(x.key));
        if (open.length) {
          test.info().annotations.push({
            type: "open exception (there before the fix round)",
            description: open.map((x) => x.text).join("; "),
          });
        }
        expect(
          problems.filter((x) => !known.has(x.key)).map((x) => x.text),
          "label problems",
        ).toEqual([]);
      });
    }
  }
});
