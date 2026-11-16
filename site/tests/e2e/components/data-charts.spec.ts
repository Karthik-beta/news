// The data and Charts I components (#8) in the browser, on their fixture pages (/fixtures/components/<Name>/<fixture>/,
// built with NEWS_FIXTURES=1). Builder's checks: the tester's full procedure and sign-off are news-component-testing.
// Pages are complete without JavaScript; only chart plots hydrate, and pages without charts ship no script; axe
// finds nothing in light or dark; each plot is named by its data summary and its table is reachable; keys, hover
// and tap drive the tooltips; no fall is ever crimson.
import { tokenValue } from "../../support/tokens";
import {
  expect,
  expectAccessible,
  expectDesignFonts,
  expectNoSidewaysScroll,
  expectNoSmallText,
  expectTargetSize,
  gotoPage,
  setTheme,
  test,
} from "../fixtures";

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
const STATIC_PAGES = PAGES.filter((p) => /^(KeyFigures?|DataTable|ComparisonTable)\//.test(p));

/** True when React has hydrated (attached a fiber to) the element. */
const isReactManaged = (el: Element) => Object.keys(el).some((k) => k.startsWith("__reactFiber$"));
const rgb = (hex: string) => {
  const [r, g, b] = [1, 3, 5].map((i) => Number.parseInt(hex.slice(i, i + 2), 16));
  return `rgb(${r}, ${g}, ${b})`;
};

for (const p of PAGES) {
  test(`${p}: no WCAG 2.2 AA violations in light and dark`, { tag: ["@smoke", "@a11y"] }, async ({ page, theme }) => {
    await gotoPage(page, url(p));
    await expectAccessible(page);
    await setTheme(page, theme === "light" ? "dark" : "light");
    await expectAccessible(page);
  });

  test(`${p}: reads at this screen size`, { tag: "@smoke" }, async ({ page }) => {
    await gotoPage(page, url(p));
    await expectNoSidewaysScroll(page);
    await expectNoSmallText(page);
    await expectTargetSize(page);
    await expectDesignFonts(page);
  });
}

for (const name of ["ChartFigure", "DataLinks", "SourcePopover", "Sparkline"]) {
  test(`${name} gallery: every case renders, accessible in light and dark`, { tag: ["@smoke", "@a11y"] }, async ({
    page,
    theme,
  }) => {
    await gotoPage(page, `/fixtures/primitives/${name}/`);
    expect(await page.locator(".fixture-case").count()).toBeGreaterThan(0);
    await expectAccessible(page);
    await setTheme(page, theme === "light" ? "dark" : "light");
    await expectAccessible(page);
    await expectNoSidewaysScroll(page);
    await expectNoSmallText(page);
  });
}

test("pages of key figures and tables ship no JavaScript", { tag: "@smoke" }, async ({ page }) => {
  for (const p of STATIC_PAGES) {
    const scripts: string[] = [];
    page.on("request", (req) => {
      if (req.resourceType() === "script") scripts.push(req.url());
    });
    await gotoPage(page, url(p));
    expect(await page.locator("script").count(), p).toBe(0);
    expect(scripts, p).toEqual([]);
    page.removeAllListeners("request");
  }
});

test("chart pages are complete without JavaScript and only the plots hydrate", { tag: "@smoke" }, async ({
  page,
  browser,
}) => {
  const noJs = await browser.newContext({ javaScriptEnabled: false });
  const raw = await noJs.newPage();
  await raw.goto(url("ColumnChart/default"));
  await expect(raw.locator(".column-chart__fill").first()).toBeVisible();
  await expect(raw.locator(".column-chart__value", { hasText: "96.2" })).toBeVisible();
  // Without the island's keys there is no dead tab stop.
  expect(await raw.locator("[role='img'][tabindex]").count()).toBe(0);
  await raw.getByText("Show the data table").first().click();
  await expect(raw.locator("figure").first().locator("tbody tr")).toHaveCount(5);
  await noJs.close();

  const warnings: string[] = [];
  page.on("console", (msg) => {
    if (/hydrat|did not match|mismatch|recoverable/i.test(msg.text())) warnings.push(msg.text());
  });
  await gotoPage(page, url("ColumnChart/default"));
  const plots = page.locator("[data-island]");
  const n = await plots.count();
  for (let i = 0; i < n; i++) {
    await plots.nth(i).scrollIntoViewIfNeeded();
    await expect(plots.nth(i).locator("[data-hydrated='true']")).toHaveCount(1);
  }
  for (const sel of ["figcaption", ".figure-frame__source", ".figure-frame__actions", ".chart-legend"]) {
    const el = page.locator(sel).first();
    if ((await el.count()) > 0) expect(await el.evaluate(isReactManaged), sel).toBe(false);
  }
  expect(warnings).toEqual([]);
});

test("each plot is named by its data summary, and its data table is reachable by keyboard", {
  tag: ["@smoke", "@a11y"],
}, async ({ page }) => {
  for (const p of ["ColumnChart/default", "BarChart/default", "LineChart/default", "StepLineChart/default"]) {
    await gotoPage(page, url(p));
    const islands = page.locator("[data-island]");
    for (let i = 0; i < (await islands.count()); i++) {
      const props = JSON.parse((await islands.nth(i).getAttribute("data-props")) ?? "{}");
      await expect(islands.nth(i).getByRole("img")).toHaveAccessibleName(props.summary);
    }
    const summary = page.locator("figure").first().getByText("Show the data table");
    await summary.focus();
    await page.keyboard.press("Enter");
    await expect(page.locator("figure").first().locator(".figure-frame__table table")).toBeVisible();
  }
});

const KEY_CASES = [
  { p: "ColumnChart/default", first: "$53.8bn", last: "$96.2bn" },
  { p: "BarChart/default", first: "$167.0bn", last: "$25.1bn" },
  { p: "LineChart/default", first: "66.0%", last: "73.9%" },
  { p: "StepLineChart/default", first: "0.25%", last: "4.50%" },
];

for (const { p, first, last } of KEY_CASES) {
  test(`${p}: arrows move between points, the tooltip follows focus, Escape closes it`, {
    tag: ["@smoke", "@interaction"],
  }, async ({ page }) => {
    await gotoPage(page, url(p));
    const island = page.locator("[data-island]").first();
    await island.scrollIntoViewIfNeeded();
    await expect(island.locator("[data-hydrated='true']")).toHaveCount(1);
    const plot = island.getByRole("img");
    await plot.focus();
    await expect(plot).toBeFocused();
    const tip = island.locator(".chart-tooltip .popover__value").first();
    await page.keyboard.press("ArrowRight");
    await expect(tip).toHaveText(first);
    await page.keyboard.press("End");
    await expect(tip).toHaveText(last);
    await expect(island.locator("[aria-live='polite']")).toContainText(last);
    await page.keyboard.press("Escape");
    await expect(island.locator(".chart-tooltip")).toHaveCount(0);
  });
}

test("a tap on a mark opens its tooltip and a tap outside closes it", { tag: ["@smoke", "@interaction"] }, async ({
  page,
  hasTouch,
}) => {
  test.skip(!hasTouch, "touch screens only");
  for (const { p, first } of KEY_CASES.slice(0, 2)) {
    await gotoPage(page, url(p));
    const island = page.locator("[data-island]").first();
    await island.scrollIntoViewIfNeeded();
    await expect(island.locator("[data-hydrated='true']")).toHaveCount(1);
    const mark = island.locator("[data-band='0'], [data-row='0']").first();
    const box = await mark.boundingBox();
    if (!box) throw new Error("no mark box");
    await page.touchscreen.tap(box.x + box.width / 2, box.y + box.height - 4);
    await expect(island.locator(".chart-tooltip .popover__value").first()).toHaveText(first);
    await page.touchscreen.tap(4, 4);
    await expect(island.locator(".chart-tooltip")).toHaveCount(0);
  }
});

test("hovering a line chart snaps the crosshair to the nearest period", { tag: "@interaction" }, async ({
  page,
  isMobile,
}) => {
  test.skip(isMobile, "no hover on touch screens");
  await gotoPage(page, url("LineChart/default"));
  const island = page.locator("[data-island]").first();
  await expect(island.locator("[data-hydrated='true']")).toHaveCount(1);
  const box = await island.getByRole("img").boundingBox();
  if (!box) throw new Error("no plot box");
  await page.mouse.move(box.x + box.width - 2, box.y + box.height / 2);
  await expect(island.locator(".line-chart__crosshair")).toHaveCount(1);
  await expect(island.locator(".chart-tooltip .popover__value").first()).toHaveText("73.9%");
});

test("no fall is shown in crimson, in either theme", { tag: "@smoke" }, async ({ page, theme }) => {
  for (const t of [theme, theme === "light" ? "dark" : "light"] as const) {
    await gotoPage(page, url("BarChart/edge-cases"));
    await setTheme(page, t);
    const crimson = rgb(tokenValue("--chart-highlight", t));
    const negatives = page.locator(".bar-chart__row:has(.is-negative) .bar-chart__bar");
    expect(await negatives.count()).toBeGreaterThan(0);
    for (const bar of await negatives.all()) {
      expect(await bar.evaluate((el) => getComputedStyle(el).backgroundColor)).not.toBe(crimson);
    }
    for (const v of await page.locator(".bar-chart__value.is-negative").all()) {
      expect(await v.evaluate((el) => getComputedStyle(el).color)).not.toBe(crimson);
    }
    await gotoPage(page, url("KeyFigures/default"));
    await setTheme(page, t);
    const ink2 = rgb(tokenValue("--color-ink-2", t));
    for (const c of await page.locator(".key-figure__change").all()) {
      expect(await c.evaluate((el) => getComputedStyle(el).color)).toBe(ink2);
    }
  }
});

test("the CSV and JSON next to a chart are its data table", { tag: "@smoke" }, async ({ page, request }) => {
  await gotoPage(page, url("BarChart/default"));
  const fig = page.locator("figure").first();
  const csvHref = await fig.getByRole("link", { name: "CSV" }).getAttribute("href");
  const csv = await (await request.get(new URL(csvHref as string, page.url()).toString())).text();
  const rows = csv.trimEnd().split("\r\n");
  expect(rows[0]).toBe("Category,fiscal 2024 (US$ millions)");
  await fig.getByText("Show the data table").click();
  const cells = await fig
    .locator("tbody tr")
    .evaluateAll((trs) =>
      trs.map((tr) => [tr.querySelector("th")?.textContent, tr.querySelector("td")?.textContent?.replaceAll(",", "")]),
    );
  expect(rows.slice(1)).toEqual(cells.map(([label, value]) => `${label},${value}`));
});
