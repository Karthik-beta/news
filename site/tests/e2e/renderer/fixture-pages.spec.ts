// The renderer's fixture pages in the browser (built with NEWS_FIXTURES=1 by `bun --bun astro build`; the
// Playwright web server sets it). The page is complete HTML before JavaScript; static parts ship 0 KB of JS and
// never hydrate; the chart island hydrates with no warnings; nothing loads from another origin (fonts included);
// citation popovers work with no script; and the page passes the shared checks on every screen.
// Fixtures: src/renderer/fixtures/bundles/. Tags: site/TESTING.md.
import { tokenValue } from "../../support/tokens";
import {
  cumulativeLayoutShift,
  expect,
  expectAccessible,
  expectDesignFonts,
  expectNoSidewaysScroll,
  expectNoSmallText,
  expectTargetSize,
  gotoPage,
  setTheme,
  test,
  watchLayoutShift,
  zoomTo200,
} from "../fixtures";

const APPLE = "/fixtures/renderer/apple-fy2024/";
const EDGE = "/fixtures/renderer/edge-cases/";
const STATIC = "/fixtures/renderer/static/";
const PAGES = { "island page": APPLE, "edge cases": EDGE, "static page": STATIC };

/** Static parts of the Apple fixture page: everything outside the chart island. */
const STATIC_PARTS = [".article__head h1", ".text-standfirst", ".page-body > p", ".fixture-figure", "figcaption"];

/** True when React has hydrated (attached a fiber to) the element. */
const isReactManaged = (el: Element) => Object.keys(el).some((k) => k.startsWith("__reactFiber$"));

test.describe("renderer fixture page with a chart island", () => {
  test("is complete without JavaScript, ships no JS for static parts, and hydrates the island cleanly", {
    tag: "@smoke",
  }, async ({ page, browser }) => {
    // 1. Without JavaScript: headline, prose, key figure, the drawn chart and its data table are all there.
    const noJs = await browser.newContext({ javaScriptEnabled: false });
    const raw = await noJs.newPage();
    await raw.goto(APPLE);
    for (const sel of STATIC_PARTS) await expect(raw.locator(sel).first()).toBeVisible();
    await expect(raw.locator(".fixture-plot__marker")).toHaveCount(5);
    await expect(raw.locator(".fixture-plot__line")).toHaveAttribute("d", /^M/);
    await raw.getByText("Show the data table").click();
    await expect(raw.locator(".figure-frame__table tbody tr")).toHaveCount(5);
    await noJs.close();

    // 2. With JavaScript: watch scripts and warnings while the page loads and hydrates.
    const warnings: string[] = [];
    page.on("console", (msg) => {
      if (/hydrat|did not match|mismatch|recoverable|warning/i.test(msg.text())) warnings.push(msg.text());
    });
    const scripts: string[] = [];
    page.on("request", (req) => {
      if (req.resourceType() === "script") scripts.push(new URL(req.url()).pathname);
    });
    await gotoPage(page, APPLE);

    // 3. The island hydrates: it marks itself live and the keyboard tooltip works (server HTML alone cannot).
    const plot = page.locator(".fixture-plot");
    await expect(plot).toHaveAttribute("data-hydrated", "true");
    await page.locator(".fixture-plot__area").focus();
    await page.keyboard.press("ArrowRight");
    await expect(page.locator(".popover--tooltip .popover__value")).toHaveText("$274.5bn");
    await page.keyboard.press("End");
    await expect(page.locator(".popover--tooltip .popover__value")).toHaveText("$391.0bn");
    await page.keyboard.press("Escape");
    await expect(page.locator(".popover--tooltip")).toHaveCount(0);

    // 4. Static parts never hydrate; every script is a built asset (none inline, none for static parts).
    expect(await plot.evaluate(isReactManaged)).toBe(true);
    for (const sel of STATIC_PARTS) expect(await page.locator(sel).first().evaluate(isReactManaged), sel).toBe(false);
    expect(scripts.length).toBeGreaterThan(0);
    for (const src of scripts) expect(src).toMatch(/^\/_astro\/[^/]+\.js$/);
    expect(warnings, "hydration warnings in the console").toEqual([]);
  });

  test("shows the tooltip on hover", { tag: "@interaction" }, async ({ page, browserName, isMobile }) => {
    test.skip(isMobile, "no hover on touch screens; tap is covered below");
    await gotoPage(page, APPLE);
    await expect(page.locator(".fixture-plot")).toHaveAttribute("data-hydrated", "true");
    const area = page.locator(".fixture-plot__area");
    await area.scrollIntoViewIfNeeded();
    const box = await area.boundingBox();
    if (!box) throw new Error(`no plot box in ${browserName}`);
    await page.mouse.move(box.x + box.width - 2, box.y + box.height / 2);
    await expect(page.locator(".popover--tooltip .popover__value")).toHaveText("$391.0bn");
  });

  test("shows the tooltip on tap and closes it on a tap outside", { tag: "@interaction" }, async ({
    page,
    hasTouch,
  }) => {
    test.skip(!hasTouch, "touch screens only");
    await gotoPage(page, APPLE);
    await expect(page.locator(".fixture-plot")).toHaveAttribute("data-hydrated", "true");
    const area = page.locator(".fixture-plot__area");
    await area.scrollIntoViewIfNeeded();
    const box = await area.boundingBox();
    if (!box) throw new Error("no plot box");
    await page.touchscreen.tap(box.x + 4, box.y + box.height / 2);
    await expect(page.locator(".popover--tooltip .popover__value")).toHaveText("$274.5bn");
    await page.touchscreen.tap(4, 4);
    await expect(page.locator(".popover--tooltip")).toHaveCount(0);
  });
});

test.describe("renderer fixture pages", () => {
  test("a page with no islands requests no JavaScript at all", { tag: "@smoke" }, async ({ page }) => {
    const scripts: string[] = [];
    page.on("request", (req) => {
      if (req.resourceType() === "script") scripts.push(req.url());
    });
    await gotoPage(page, STATIC);
    await expect(page.getByRole("heading", { level: 1 })).toHaveText("A static page ships no JavaScript");
    expect(scripts).toEqual([]);
    expect(await page.locator("script").count()).toBe(0);
  });

  for (const [name, path] of Object.entries(PAGES)) {
    test(`${name}: makes no third-party requests and loads the self-hosted fonts`, { tag: "@smoke" }, async ({
      page,
      baseURL,
    }) => {
      const requests: string[] = [];
      page.on("request", (req) => {
        if (!req.url().startsWith("data:")) requests.push(req.url());
      });
      await gotoPage(page, path);
      await page.evaluate(() => document.fonts.ready.then(() => undefined));
      const origin = new URL(baseURL as string).origin;
      const foreign = requests.filter((u) => new URL(u).origin !== origin);
      expect(foreign, "requests to other origins").toEqual([]);
      expect(requests.some((u) => /\/fonts\/literata-latin-opsz-normal\.woff2$/.test(u))).toBe(true);
      expect(requests.some((u) => /\/fonts\/fira-sans-latin-\d00-normal\.woff2$/.test(u))).toBe(true);
      const loaded = await page.evaluate(() =>
        [...document.fonts].filter((f) => f.status === "loaded").map((f) => f.family.replace(/["']/g, "")),
      );
      expect(new Set(loaded)).toEqual(new Set(["Literata", "Fira Sans"]));
    });

    test(`${name}: has no WCAG 2.2 AA violations`, { tag: ["@smoke", "@a11y"] }, async ({ page, theme }) => {
      await gotoPage(page, path);
      await expectAccessible(page);
      await setTheme(page, theme === "light" ? "dark" : "light");
      await expectAccessible(page);
    });

    test(`${name}: reads at this screen size`, async ({ page }) => {
      await gotoPage(page, path);
      await expect(page.getByRole("heading", { level: 1 })).toHaveCount(1);
      await expectNoSidewaysScroll(page);
      await expectNoSmallText(page);
      await expectTargetSize(page);
      await expectDesignFonts(page);
    });

    test(`${name}: reflows at 200% zoom`, { tag: "@zoom" }, async ({ page }) => {
      await gotoPage(page, path);
      test.skip(!(await zoomTo200(page)), "below 640px wide, 200% zoom is the 320 project (400%)");
      await expectNoSidewaysScroll(page);
      await expectNoSmallText(page);
    });

    test(`${name}: prints the article and its figures`, { tag: "@print" }, async ({ page }) => {
      await gotoPage(page, path);
      await page.emulateMedia({ media: "print" });
      await expect(page.getByRole("heading", { level: 1 })).toBeVisible();
      await expect(page.locator(".article__sources")).toBeVisible();
      for (const actions of await page.locator(".figure-frame__actions").all()) await expect(actions).toBeHidden();
    });

    test(`${name}: keeps structure and the chart tag in forced colours`, { tag: "@forced-colors" }, async ({
      page,
    }) => {
      await gotoPage(page, path);
      await expect(page.getByRole("heading", { level: 1 })).toBeVisible();
      await expectAccessible(page);
    });
  }

  test("a citation popover opens from the cited figure with no script, and Escape closes it", {
    tag: ["@smoke", "@interaction"],
  }, async ({ page }) => {
    await gotoPage(page, STATIC);
    const trigger = page.locator(".page-body button.source-ref").first();
    await expect(trigger).toHaveText("$391.0bn");
    const panel = page.locator(`#${await trigger.getAttribute("popovertarget")}`);
    await expect(panel).toBeHidden();
    await trigger.click();
    await expect(panel).toBeVisible();
    await expect(panel.locator(".popover__value")).toHaveText("$391,035 million");
    await expect(panel.getByRole("link", { name: "Open the source" })).toHaveAttribute(
      "href",
      /^https:\/\/www\.sec\.gov\//,
    );
    await page.keyboard.press("Escape");
    await expect(panel).toBeHidden();
    await expect(trigger).toBeFocused();
    // Keyboard: Enter on the focused trigger opens it again.
    await page.keyboard.press("Enter");
    await expect(panel).toBeVisible();
  });

  test("shows missing data as 'No data', never as zero", { tag: "@smoke" }, async ({ page }) => {
    await gotoPage(page, EDGE);
    const figure = (label: string) =>
      page
        .locator(".fixture-figure", { has: page.getByText(label, { exact: true }) })
        .locator("dd.text-figure > :first-child");
    await expect(figure("Not published")).toHaveText("No data");
    await expect(figure("Unknown reference")).toHaveText("No data");
    await expect(figure("Zero")).toHaveText("0%");
  });

  test("a performance check like Lighthouse's: no shift after fonts load, fast LCP, small JS", {
    tag: "@smoke",
  }, async ({ page, browserName }) => {
    let jsBytes = 0;
    page.on("response", async (res) => {
      if (res.request().resourceType() === "script") jsBytes += (await res.body()).length;
    });
    await gotoPage(page, APPLE);
    const shiftSinceFonts = await watchLayoutShift(page);
    await expect(page.locator(".fixture-plot")).toHaveAttribute("data-hydrated", "true");
    const total = await cumulativeLayoutShift(page);
    test.skip(total === null, `${browserName} does not report layout shifts`);
    // Hydration and late content must not move the page (Lighthouse shows CLS to three decimals, so under 0.001
    // reads as 0). The CLS since navigation also counts the font swap (font-display: swap, design/typography.md),
    // which depends on whether the fonts beat first paint; it is recorded here, not asserted (see TESTING.md).
    test.info().annotations.push({ type: "CLS since navigation, font swap included", description: String(total) });
    expect(await shiftSinceFonts(), "layout shift after the fonts loaded (hydration, late content)").toBeLessThan(
      0.001,
    );
    const timing = await page.evaluate(
      () =>
        new Promise<{ lcp: number; fcp: number; dcl: number }>((resolve) => {
          const nav = performance.getEntriesByType("navigation")[0] as PerformanceNavigationTiming;
          const fcp = performance.getEntriesByName("first-contentful-paint")[0]?.startTime ?? 0;
          new PerformanceObserver((list) => {
            const entries = list.getEntries();
            resolve({ lcp: entries.at(-1)?.startTime ?? 0, fcp, dcl: nav.domContentLoadedEventEnd });
          }).observe({ type: "largest-contentful-paint", buffered: true });
        }),
    );
    // Local preview server, no throttling: these are regression budgets, not field targets.
    expect(timing.fcp, "first contentful paint (ms)").toBeLessThan(1800);
    expect(timing.lcp, "largest contentful paint (ms)").toBeLessThan(2500);
    expect(jsBytes, "the island page loads its JavaScript").toBeGreaterThan(0);
    // Uncompressed bytes as served by `astro preview`: the island page's whole JS (React DOM + island).
    expect(jsBytes / 1024, "JavaScript KB (uncompressed)").toBeLessThan(260);
  });

  test("dark theme comes from the tokens", { tag: "@smoke" }, async ({ page, theme }) => {
    await gotoPage(page, APPLE);
    const paper = await page.evaluate(() => getComputedStyle(document.body).backgroundColor);
    const expected = tokenValue("--color-paper", theme);
    const [r, g, b] = [1, 3, 5].map((i) => Number.parseInt(expected.slice(i, i + 2), 16));
    expect(paper).toBe(`rgb(${r}, ${g}, ${b})`);
  });
});

test.describe("primitive gallery pages", () => {
  const NAMES = ["FigureFrame", "Icon", "Popover", "Rule", "SourceRef", "Text", "VisuallyHidden"];
  for (const name of NAMES) {
    test(`${name}: renders every case, accessible and within the screen`, { tag: "@a11y" }, async ({ page }) => {
      await gotoPage(page, `/fixtures/primitives/${name}/`);
      await expect(page.getByRole("heading", { level: 1 })).toHaveText(name);
      expect(await page.locator(".fixture-case").count()).toBeGreaterThan(0);
      await expectAccessible(page);
      await expectNoSidewaysScroll(page);
      await expectNoSmallText(page);
      await expectDesignFonts(page);
    });
  }

  test("all galleries pass axe", { tag: ["@smoke", "@a11y"] }, async ({ page }) => {
    for (const name of NAMES) {
      await gotoPage(page, `/fixtures/primitives/${name}/`);
      await expectAccessible(page);
    }
  });
});
