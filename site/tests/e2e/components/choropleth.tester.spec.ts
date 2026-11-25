// Component tester's browser checks for the ChoroplethMap (#13), on top of the builder's choropleth.spec.ts. Every
// fixture page on every screen: nothing clipped, off screen or overlapping (the legend's break labels above all), at
// 100% and 200% text and 200% zoom; the narrow/wide switch at exactly 36em of the figure's own text (ranked bars under
// a small map, or the full map and the ranked table); the hatch on regions with no data drawn at least one device
// pixel wide at every map size; every region and swatch the token the spec names in both themes (dark reverses the
// ramp), by system setting and by data-theme; forced colours and print keep the shades; the accessible names (the
// summary once, "Ranked: <title>" for the ranked view, no two tables of one name with the data table open); one tab
// stop, on the bars; no motion and no shift; scripts blocked; and the map in context in the charts article fixture.
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

const PAGES = ["ChoroplethMap/default", "ChoroplethMap/edge-cases"];
const url = (p: string) => `/fixtures/components/${p}/`;
const FLOATING = "[popover], .popover--tooltip, .chart-tooltip";
const LIGHT = [300, 400, 500, 600, 700];
const DARK = [600, 400, 300, 200, 100];

/** Per map: legend labels that overlap each other or leave the figure, and the legend's own overflow. */
const legendProblems = (page: Page) =>
  page.locator(".choropleth").evaluateAll((maps) =>
    maps.flatMap((map) => {
      const box = map.getBoundingClientRect();
      const name = map.closest("figure")?.querySelector("h2")?.id ?? "?";
      const labels = [
        ...map.querySelectorAll<HTMLElement>(".choropleth__edge, .choropleth__legend .chart-legend__item"),
      ]
        .map((el) => ({ text: el.textContent, r: el.getBoundingClientRect() }))
        .filter((l) => l.r.width > 0);
      const out: string[] = [];
      for (const l of labels) {
        if (l.r.left < box.left - 0.5 || l.r.right > box.right + 0.5)
          out.push(`${name}: "${l.text}" leaves the figure`);
      }
      for (let i = 0; i < labels.length; i++) {
        for (let j = i + 1; j < labels.length; j++) {
          const [a, b] = [labels[i], labels[j]] as [(typeof labels)[number], (typeof labels)[number]];
          const x = Math.min(a.r.right, b.r.right) - Math.max(a.r.left, b.r.left);
          const y = Math.min(a.r.bottom, b.r.bottom) - Math.max(a.r.top, b.r.top);
          if (x > 0.5 && y > 0.5) out.push(`${name}: "${a.text}" × "${b.text}"`);
        }
      }
      return out;
    }),
  );

/** Per map: its width in its own em, and which of the ranked views shows. */
const views = (page: Page) =>
  page.locator(".choropleth").evaluateAll((maps) =>
    maps.map((map) => {
      const shown = (sel: string) => (map.querySelector(sel) as HTMLElement).offsetParent !== null;
      const em = Number.parseFloat(getComputedStyle(map).fontSize);
      return {
        name: map.closest("figure")?.querySelector("h2")?.id,
        ems: map.getBoundingClientRect().width / em,
        table: shown(".choropleth__table"),
        bars: shown(".choropleth__bars"),
        map: (map.querySelector("svg.choropleth__map") as SVGSVGElement).getBoundingClientRect().width,
      };
    }),
  );

async function expectReads(page: Page, textSize: number): Promise<void> {
  await expectNoSidewaysScroll(page);
  await expectInsideViewport(page, page.locator("main"), FLOATING);
  expect(await legendProblems(page), `legend at ${textSize}% text`).toEqual([]);
  for (const v of await views(page)) {
    const wide = v.ems >= 36 - 0.01;
    expect({ table: v.table, bars: v.bars }, `${v.name} at ${v.ems.toFixed(1)}em, ${textSize}% text`).toEqual({
      table: wide,
      bars: !wide,
    });
  }
}

for (const p of PAGES) {
  test(`${p}: nothing clipped, off screen or overlapping; the ranked view the figure's width calls for`, async ({
    page,
  }) => {
    await gotoPage(page, url(p));
    await expectReads(page, 100);
    await expectNoClippedText(page);
  });

  test(`${p}: reflows at 200% text size and 200% zoom, the legend inside the figure`, { tag: "@zoom" }, async ({
    page,
  }) => {
    await gotoPage(page, url(p));
    await setTextSize(page, 200);
    await expectReads(page, 200);
    await setTextSize(page, 100);
    test.skip(!(await zoomTo200(page)), "below 640px wide, 200% zoom is the 320 project (400%)");
    await expectReads(page, 100);
  });

  test(`${p}: the no-data hatch is at least one device pixel wide at this map size`, async ({ page }) => {
    await gotoPage(page, url(p));
    const dpr = await page.evaluate(() => devicePixelRatio);
    for (const svg of await page.locator("svg.choropleth__map").all()) {
      const h = await svg.evaluate((el) => {
        const line = el.querySelector(".choropleth__hatch") as SVGLineElement;
        const pattern = el.querySelector("pattern") as SVGPatternElement;
        const k = el.getBoundingClientRect().width / (el as SVGSVGElement).viewBox.baseVal.width;
        return {
          width: Number.parseFloat(getComputedStyle(line).strokeWidth) * k,
          period: pattern.width.baseVal.value * k,
          stroke: getComputedStyle(line).stroke,
          missing: el.querySelectorAll("path.is-missing").length,
        };
      });
      if (h.missing === 0) continue;
      expect(h.width * dpr, `hatch line ${h.width.toFixed(2)}px at ${dpr}x`).toBeGreaterThanOrEqual(1);
      expect(h.period / h.width, "lines apart, not a flat fill").toBeGreaterThanOrEqual(3);
      expect(h.stroke).not.toBe("none");
    }
  });

  test(`${p}: nothing animates on load and nothing moves once the fonts are in`, async ({ page }) => {
    await gotoPage(page, url(p));
    const shift = await watchLayoutShift(page);
    expect(await runningAnimations(page)).toEqual([]);
    expect(await longestMotionMs(page), "no motion under reduced motion").toBe(0);
    if ((await cumulativeLayoutShift(page)) !== null) {
      expect(await shift(), "layout shift after the fonts loaded").toBeLessThan(0.001);
    }
  });

  test(`${p}: transitions stay within 200 ms when motion is allowed`, { tag: "@motion" }, async ({ page }) => {
    await gotoPage(page, url(p));
    expect(await runningAnimations(page)).toEqual([]);
    expect(await longestMotionMs(page)).toBeLessThanOrEqual(200);
  });

  test(`${p}: forced colours keep the shades, the hatch and the borders, and pass axe`, {
    tag: "@forced-colors",
  }, async ({ page }) => {
    await gotoPage(page, url(p));
    expect(await page.evaluate(() => matchMedia("(forced-colors: active)").matches)).toBe(true);
    await expectAccessible(page);
    const m = await page
      .locator("svg.choropleth__map")
      .first()
      .evaluate((svg) => ({
        fills: new Set([...svg.querySelectorAll("path[data-ref]")].map((p) => getComputedStyle(p).fill)).size,
        steps: new Set([...svg.querySelectorAll("path[data-ref]")].map((p) => p.getAttribute("class"))).size,
        hatch: getComputedStyle(svg.querySelector(".choropleth__hatch") as Element).stroke,
        missing: getComputedStyle(svg.querySelector("path.is-missing") as Element).fill,
      }));
    expect(m.fills, "one fill per step").toBe(m.steps);
    expect(m.hatch).not.toBe("none");
    expect(m.missing).toMatch(/^url\(/);
  });

  test(`${p}: prints with its shades and hatch`, { tag: "@print" }, async ({ page }) => {
    await gotoPage(page, url(p));
    await page.emulateMedia({ media: "print" });
    const unprinted = await page
      .locator("svg.choropleth__map, .choropleth__swatch, .choropleth .chart-hatch")
      .evaluateAll((els) =>
        els.filter((el) => getComputedStyle(el).printColorAdjust !== "exact").map((el) => el.getAttribute("class")),
      );
    expect(unprinted).toEqual([]);
  });

  test(`${p}: with scripts blocked, the map, legend and ranked data are all there`, async ({ browser, baseURL }) => {
    const context = await browser.newContext({ javaScriptEnabled: false, baseURL });
    const page = await context.newPage();
    await page.goto(url(p));
    for (const fig of await page.locator("figure:has(.choropleth)").all()) {
      await expect(fig.locator("svg.choropleth__map path").first()).toBeAttached();
      await expect(fig.locator(".choropleth__class").first()).toBeVisible();
      const rows = await fig.locator(".choropleth__table tbody tr").count();
      await expect(fig.locator(".choropleth__bars .bar-chart__row")).toHaveCount(rows);
    }
    await context.close();
  });
}

test("every region and legend swatch is the token for its step, in both themes, by system setting and data-theme", {
  tag: "@smoke",
}, async ({ page, theme }) => {
  await gotoPage(page, url("ChoroplethMap/default"));
  for (const t of ["system", "light", "dark"] as const) {
    await setTheme(page, t);
    const shown = t === "system" ? theme : t;
    const ramp = shown === "dark" ? DARK : LIGHT;
    const colours = await page.locator(".choropleth").evaluateAll((maps) =>
      maps.flatMap((map) => [
        ...[...map.querySelectorAll("path[data-ref]")].map((el) => ({
          el: "region",
          cls: el.getAttribute("class"),
          c: getComputedStyle(el).fill,
        })),
        ...[...map.querySelectorAll(".choropleth__swatch")].map((el) => ({
          el: "swatch",
          cls: el.className,
          c: getComputedStyle(el).backgroundColor,
        })),
      ]),
    );
    expect(colours.length).toBeGreaterThan(100);
    for (const c of colours) {
      const step = Number(/heatmap__step--(\d)/.exec(c.cls ?? "")?.[1]);
      expect(c.c, `${t} (${shown}) ${c.el} step ${step}`).toBe(
        rgb(tokenValue(`--chart-seq-blue-${ramp[step - 1]}`, shown)),
      );
    }
    // Borders in the paper colour; the hatch and the outline of a region with no data in chart.neutral.
    const lines = await page
      .locator("svg.choropleth__map")
      .first()
      .evaluate((svg) => ({
        border: getComputedStyle(svg.querySelector("path[data-ref]") as Element).stroke,
        hatch: getComputedStyle(svg.querySelector(".choropleth__hatch") as Element).stroke,
        outline: getComputedStyle(svg.querySelector("path.is-missing") as Element).stroke,
      }));
    expect(lines).toEqual({
      border: rgb(tokenValue("--color-paper", shown)),
      hatch: rgb(tokenValue("--chart-neutral", shown)),
      outline: rgb(tokenValue("--chart-neutral", shown)),
    });
  }
});

test("the summary is read once, the ranked view is 'Ranked: <title>', and no two tables share a name", {
  tag: ["@smoke", "@a11y"],
}, async ({ page, browserName }) => {
  await gotoPage(page, url("ChoroplethMap/default"));
  const fig = page.locator("figure", { has: page.locator("#g7-title") });
  const title = (await page.locator("#g7-title").textContent()) ?? "";
  await fig.locator(".figure-frame__table summary").click();
  await expectAccessible(page);
  // Playwright's Firefox crashes the page when axe and ariaSnapshot() both run with the <details> open (each alone is
  // fine): a tooling crash, not the page (SIGNOFF.md). The names are checked in Chromium and WebKit.
  test.skip(browserName === "firefox", "Playwright Firefox crashes on ariaSnapshot after axe here");
  const snapshot = await fig.ariaSnapshot();
  const images = [...snapshot.matchAll(/img "([^"]+)"/g)].map((m) => m[1]);
  const tables = [...snapshot.matchAll(/table "([^"]+)"/g)].map((m) => m[1]);
  expect(images.length, snapshot).toBeGreaterThanOrEqual(1);
  expect(new Set(images).size, "no two images of one name").toBe(images.length);
  expect(new Set(tables).size, "no two tables of one name").toBe(tables.length);
  expect(images[0]).toMatch(
    new RegExp(`^${title.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}\\. Unemployment rate: France, 7\\.4%`),
  );
  if (await fig.locator(".choropleth__table").isVisible()) expect(tables).toContain(`Ranked: ${title}`);
  else expect(images).toContain(`Ranked: ${title}`);
});

test("one tab stop per map, on the ranked bars when they show; the arrow keys step through them", {
  tag: "@interaction",
}, async ({ page }) => {
  await gotoPage(page, url("ChoroplethMap/default"));
  const fig = page.locator("figure", { has: page.locator("#g7-title") });
  await expect(fig.locator("svg.choropleth__map")).not.toHaveAttribute("tabindex");
  const bars = fig.locator(".choropleth__bars [role='img']");
  if (!(await bars.isVisible())) {
    // Wide: the map and table take no stop of their own; the figure's stops are the toggle and downloads.
    await expect(fig.locator(".choropleth [tabindex='0']:visible")).toHaveCount(0);
    return;
  }
  await bars.scrollIntoViewIfNeeded();
  await expect(fig.locator(".choropleth__bars [data-hydrated='true']")).toHaveCount(1);
  await bars.focus();
  await page.keyboard.press("Home");
  const tip = fig.locator(".chart-tooltip");
  await expect(tip).toContainText("France");
  await page.keyboard.press("ArrowDown");
  await expect(tip).toContainText("Italy");
  await expectInsideViewport(page, page.locator("main"));
});

test.describe("in context: the charts article fixture", () => {
  test("the G7 map reads in the article, between the heatmap and the sources", async ({ page }) => {
    await gotoPage(page, "/fixtures/renderer/charts/");
    const fig = page.locator("figure", { has: page.locator("#g7map-title") });
    await fig.scrollIntoViewIfNeeded();
    await expectReads(page, 100);
    await expectNoClippedText(page);
    // The figure keeps the article's rhythm: the gap above it equals the gap above the heatmap before it.
    const gaps = await page.evaluate(() => {
      const gapAbove = (id: string) => {
        const fig = document.getElementById(id)?.closest("figure") as HTMLElement;
        const prev = fig.previousElementSibling as HTMLElement;
        return Math.round(fig.getBoundingClientRect().top - prev.getBoundingClientRect().bottom);
      };
      return [gapAbove("marginheat-title"), gapAbove("g7map-title")];
    });
    expect(gaps[1]).toBe(gaps[0]);
    await setTextSize(page, 200);
    await expectReads(page, 200);
  });
});
