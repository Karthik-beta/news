// Tester's browser checks for the SSR page templates of #14 (src/views/Views.tsx in src/views/Shell.astro), served by
// the built Worker (wrangler dev) over the fixture curations: home (normal, withdrawn lead, region set, empty Latest),
// region view, Latest and its Older page, category and sub-category hubs, the region filter, an empty filtered hub,
// the entity hub, search (form, results, empty) and the 404. Untagged tests run on every screen (site/TESTING.md).
// The search daily-limit page needs D1 to fail, which wrangler's local D1 cannot: its markup is axe-checked in
// tests/component/ssr-templates.tester.test.tsx and its status and headers in src/server/serve.test.ts.
import type { Page } from "@playwright/test";
import {
  cumulativeLayoutShift,
  expect,
  expectAccessible,
  expectDesignFonts,
  expectNoSidewaysScroll,
  expectNoSmallText,
  expectTargetSize,
  expectTheme,
  gotoPage,
  longestMotionMs,
  runningAnimations,
  setTextSize,
  setTheme,
  test,
  watchLayoutShift,
  zoomTo200,
} from "./fixtures";
import { type FixtureSet, setOrigin } from "./ssr-servers";

interface Template {
  name: string;
  path: string;
  set?: FixtureSet;
  status?: number;
}

const TEMPLATES: Template[] = [
  { name: "home", path: "/" },
  { name: "home, withdrawn lead", path: "/", set: "withdrawn-lead" },
  { name: "home, region set", path: "/", set: "region" },
  { name: "home, empty Latest", path: "/", set: "empty-latest" },
  { name: "region view", path: "/region/india", set: "region" },
  { name: "Latest", path: "/latest" },
  { name: "category hub", path: "/economy" },
  { name: "sub-category hub", path: "/economy/inflation" },
  { name: "category hub, region filter", path: "/economy?region=north-america" },
  { name: "category hub, empty region", path: "/finance?region=australia" },
  { name: "entity hub", path: "/entity/company%3Aapple" },
  { name: "search form", path: "/search" },
  { name: "search results", path: "/search?q=inflation" },
  { name: "search, no results", path: "/search?q=zzzzqqqq" },
  { name: "404", path: "/no-such-section", status: 404 },
  // The prerendered static pages: static-pages.spec.ts checks them in the system theme; here both themes, zoom and print.
  ...["methodology", "about", "glossary", "privacy", "legal", "contact", "report", "data", "corrections", "feeds"].map(
    (p) => ({ name: `static page ${p}`, path: `/${p}/` }),
  ),
];

/** Behaviour that does not change with the screen is checked at a phone and a desktop width only. */
const BEHAVIOUR_WIDTHS = [360, 1440];

const url = (t: Template) => (t.set ? `${setOrigin(t.set)}${t.path}` : t.path);
const opposite = (t: "light" | "dark") => (t === "light" ? "dark" : "light");

for (const t of TEMPLATES) {
  test.describe(`SSR template: ${t.name} (${t.set ?? "normal"} ${t.path})`, () => {
    // The 404 page's own document answers 404, which the console guard would otherwise count as a failure.
    test.use({ allowedConsoleErrors: t.status === 404 ? [/HTTP 404|status of 404/] : [] });

    test.beforeEach(async ({ page }) => {
      const res = await page.goto(url(t));
      expect(res?.status()).toBe(t.status ?? 200);
      await page.evaluate(() => document.fonts.ready.then(() => undefined));
    });

    // One page load for the screen, both themes and 200% text: the matrix runs this on every width, so a load per
    // check would triple its time on the shared laptop.
    test("reads at this screen size, has no WCAG 2.2 AA violations in either theme, keeps content at 200% text", {
      tag: ["@smoke", "@a11y", "@zoom"],
    }, async ({ page, theme }) => {
      await expect(page.getByRole("heading", { level: 1 })).toHaveCount(1);
      await expectNoSidewaysScroll(page);
      await expectNoSmallText(page);
      await expectTargetSize(page);
      await expectDesignFonts(page);
      await expectNothingClipped(page);

      await expectAccessible(page);
      await setTheme(page, opposite(theme));
      await expectTheme(page, opposite(theme));
      await expectAccessible(page);
      await setTheme(page, "system");

      // biome-ignore lint/performance/useDomNodeTextContent: rendered text (innerText) is what must not be lost
      const before = await page.locator("main").innerText();
      await setTextSize(page, 200);
      await expect(page.locator("main")).toHaveText(before, { useInnerText: true });
      await expectNoSidewaysScroll(page);
      await expectNothingClipped(page);

      // 200% browser zoom halves the CSS viewport; below 640px wide the 320 project is that case (400%).
      if (await zoomTo200(page)) {
        await setTextSize(page, 100);
        await expectNoSidewaysScroll(page);
        await expectNoSmallText(page);
      }
    });

    test("prints without navigation, filters or search, keeping the stories", { tag: "@print" }, async ({ page }) => {
      const headlines = await page.locator("main :is(h2, h3) a").count();
      await page.emulateMedia({ media: "print" });
      for (const sel of ["nav", ".region-filter", ".pagination", ".masthead__search", ".site-footer__links"])
        for (const el of await page.locator(sel).all()) await expect(el).toBeHidden();
      await expect(page.locator("main :is(h2, h3) a:visible")).toHaveCount(headlines);
      // Print is on white paper in light colours (design/layout.md "Print").
      const bg = await page.evaluate(() => getComputedStyle(document.body).backgroundColor);
      expect(bg).toMatch(/rgba\(0, 0, 0, 0\)|transparent/);
    });

    test("keeps text and structure in forced colours", { tag: "@forced-colors" }, async ({ page }) => {
      await expect(page.getByRole("heading", { level: 1 })).toHaveCount(1);
      await expectAccessible(page);
    });

    test("never animates on load, and transitions stay within 200ms", { tag: "@motion" }, async ({ page }) => {
      expect(await runningAnimations(page)).toEqual([]);
      expect(await longestMotionMs(page)).toBeLessThanOrEqual(200);
    });
  });
}

/** No visible text box sticks out of the viewport or overlaps another story's headline. */
async function expectNothingClipped(page: Page) {
  const problems = await page.evaluate(() => {
    const out: string[] = [];
    const vw = document.documentElement.clientWidth;
    for (const el of document.querySelectorAll<HTMLElement>("main h1, main h2, main h3, main p, main a, main li")) {
      const r = el.getBoundingClientRect();
      if (r.width === 0 || r.height === 0 || r.width <= 1) continue;
      if (r.right > vw + 0.5 || r.left < -0.5) out.push(`${el.tagName} off screen: "${el.textContent?.slice(0, 30)}"`);
    }
    const heads = [...document.querySelectorAll<HTMLElement>("main :is(h2, h3) a")].map((a) => ({
      a,
      r: a.getBoundingClientRect(),
    }));
    heads.forEach(({ a: p, r: x }, i) => {
      for (const { a: q, r: y } of heads.slice(i + 1))
        if (x.width && y.width && x.left < y.right && y.left < x.right && x.top < y.bottom && y.top < x.bottom)
          out.push(`headlines overlap: "${p.textContent}" / "${q.textContent}"`);
    });
    return out;
  });
  expect(problems).toEqual([]);
}

test.describe("SSR templates: layout per breakpoint", () => {
  test("home lead and side stories: two columns from 820px, stacked below (design/layout.md)", async ({ page }) => {
    await gotoPage(page, "/");
    const width = page.viewportSize()?.width ?? 0;
    const main = await page.locator(".home-lead__main").boundingBox();
    const side = await page.locator(".home-lead__side").boundingBox();
    if (!main || !side) throw new Error("no lead block");
    if (width >= 820) {
      expect(Math.abs(main.y - side.y)).toBeLessThan(2);
      expect(side.x).toBeGreaterThan(main.x + main.width - 1);
      // 1.7fr / 1fr, with the hairline and padding inside the columns.
      expect(main.width / side.width).toBeGreaterThan(1.4);
    } else {
      expect(side.y).toBeGreaterThanOrEqual(main.y + main.height - 1);
    }
  });

  test("home section rows: stories at least 220px wide, side by side when they fit", async ({ page }) => {
    await gotoPage(page, "/");
    const rows = await page.locator(".home-section__row").evaluateAll((els) =>
      els.map((row) => {
        const kids = [...row.children].map((c) => c.getBoundingClientRect());
        return { width: row.getBoundingClientRect().width, kids: kids.map((k) => ({ x: k.x, w: k.width })) };
      }),
    );
    expect(rows.length).toBeGreaterThan(0);
    for (const row of rows) {
      const across = new Set(row.kids.map((k) => Math.round(k.x))).size;
      const fit = Math.max(1, Math.min(row.kids.length, Math.floor((row.width + 40) / (220 + 40))));
      expect(across, `row ${Math.round(row.width)}px wide`).toBe(fit);
      for (const k of row.kids) expect(k.w).toBeGreaterThanOrEqual(Math.min(220, row.width) - 1);
    }
  });
});

test.describe("SSR templates: what each shows", () => {
  test.skip(
    ({ viewport }) => !BEHAVIOUR_WIDTHS.includes(viewport?.width ?? 0),
    "behaviour, not layout: at 360 and 1440",
  );
  test("home: the lead, Latest list and sections; nothing empty", { tag: "@smoke" }, async ({ page }) => {
    await gotoPage(page, "/");
    await expect(page.getByRole("heading", { level: 1, name: "Front page" })).toBeAttached();
    await expect(page.locator(".home-lead__main h2 a")).toHaveText(
      "Apple reports its results for the 2024 fiscal year",
    );
    await expect(page.getByRole("link", { name: /latest/i }).first()).toHaveAttribute("href", "/latest");
    expect(await page.locator(".home-section").count()).toBeGreaterThanOrEqual(3);
    for (const row of await page.locator(".home-section__row").all())
      expect(await row.locator("article, .story-item").count()).toBeGreaterThan(0);
    await expect(page.locator('meta[name="robots"]')).toHaveAttribute("content", "index, follow");
    await expect(page.locator('link[rel="canonical"]')).toHaveAttribute("href", "https://news.karthik.nz/");
  });

  test("home, withdrawn lead: back-filled from the section, the withdrawn page nowhere", async ({ page }) => {
    await gotoPage(page, `${setOrigin("withdrawn-lead")}/`);
    await expect(page.locator(".home-lead__main h2 a")).toHaveText("European lenders open the earnings season");
    await expect(page.getByText("Apple reports its results for the 2024 fiscal year")).toHaveCount(0);
  });

  test("home, empty Latest: no Latest strip and no empty message", async ({ page }) => {
    await gotoPage(page, `${setOrigin("empty-latest")}/`);
    await expect(page.locator(".latest-list")).toHaveCount(0);
    await expect(page.locator(".state-message")).toHaveCount(0);
    await expect(page.locator(".home-lead__main h2 a")).toHaveCount(1);
  });

  test("region view: the region is current in the filter and older pages are a link away", async ({ page }) => {
    await gotoPage(page, `${setOrigin("region")}/region/india`);
    await expect(page.getByRole("heading", { level: 1, name: "News from India" })).toBeAttached();
    const filter = page.getByRole("navigation", { name: "Regions" });
    await expect(filter.locator('[aria-current="page"]')).toHaveText("India");
    expect(await page.locator("main article").count()).toBeLessThanOrEqual(12);
    await page.getByRole("link", { name: "Older" }).click();
    await expect(page).toHaveURL(/\/region\/india\?after=/);
    await expect(page.locator('meta[name="robots"]')).toHaveAttribute("content", "noindex, follow");
    expect(await page.locator("main article").count()).toBeGreaterThan(0);
  });

  test("the region filter on home moves to the region view", { tag: "@interaction" }, async ({ page }) => {
    await gotoPage(page, "/");
    await page.getByRole("navigation", { name: "Regions" }).getByRole("link", { name: "Europe" }).click();
    await expect(page).toHaveURL(/\/region\/europe$/);
    await expect(page.getByRole("heading", { level: 1, name: "News from Europe" })).toBeAttached();
  });

  test("Latest pages at 20, Older gives the rest, and the cursor page is noindex", async ({ page }) => {
    await gotoPage(page, "/latest");
    await expect(page.locator("main .story-list > li")).toHaveCount(20);
    await page.getByRole("link", { name: "Older" }).click();
    await expect(page).toHaveURL(/\/latest\?after=/);
    await expect(page.locator("main .story-list > li")).toHaveCount(9);
    await expect(page.getByRole("link", { name: "Older" })).toHaveCount(0);
    await expect(page.locator('meta[name="robots"]')).toHaveAttribute("content", "noindex, follow");
  });

  test("category hub: sub-categories, region filter and a dated list", async ({ page }) => {
    await gotoPage(page, "/economy");
    await expect(page.getByRole("heading", { level: 1, name: "Economy" })).toBeVisible();
    await page.getByRole("navigation", { name: "Sub-categories" }).getByRole("link", { name: "Inflation" }).click();
    await expect(page).toHaveURL(/\/economy\/inflation$/);
    await expect(page.getByRole("heading", { level: 1, name: "Inflation" })).toBeVisible();
    for (const li of await page.locator("main .story-list > li").all()) await expect(li.locator("time")).toHaveCount(1);
  });

  test("region filter on a hub keeps only that region and is noindex", async ({ page }) => {
    await gotoPage(page, "/economy?region=north-america");
    const meta = await page.locator("main .story-item__meta").allInnerTexts();
    expect(meta.length).toBeGreaterThan(0);
    for (const m of meta) expect(m).toContain("North America");
    await expect(page.locator('meta[name="robots"]')).toHaveAttribute("content", "noindex, follow");
    await gotoPage(page, "/finance?region=australia");
    await expect(page.getByText("There are no pages in this region yet.")).toBeVisible();
    await expect(page.getByRole("link", { name: "Show all regions" })).toHaveAttribute("href", "/finance");
  });

  test("entity hub: the latest-figures strip and pages grouped by year", async ({ page }) => {
    await gotoPage(page, "/entity/company%3Aapple");
    await expect(page.getByRole("region", { name: "Latest figures" })).toBeVisible();
    await expect(page.getByRole("heading", { level: 2, name: "2025" })).toBeVisible();
  });
});

test.describe("SSR templates: the 404", () => {
  test.skip(
    ({ viewport }) => !BEHAVIOUR_WIDTHS.includes(viewport?.width ?? 0),
    "behaviour, not layout: at 360 and 1440",
  );
  test.use({ allowedConsoleErrors: [/HTTP 404|status of 404/] });

  test("a 404 status, noindex, and a search form", async ({ page }) => {
    const res = await page.goto("/no-such-section");
    expect(res?.status()).toBe(404);
    await expect(page.locator('meta[name="robots"]')).toHaveAttribute("content", "noindex, follow");
    await expect(page.getByRole("search")).toBeVisible();
  });
});

test.describe("SSR templates without JavaScript", () => {
  test.skip(
    ({ viewport }) => !BEHAVIOUR_WIDTHS.includes(viewport?.width ?? 0),
    "behaviour, not layout: at 360 and 1440",
  );
  test.use({ javaScriptEnabled: false });

  for (const path of ["/", "/economy", "/entity/company%3Aapple", "/search?q=inflation"]) {
    test(`${path} is complete static HTML, with no script at all`, { tag: "@smoke" }, async ({ page }) => {
      await page.goto(path);
      await expect(page.getByRole("heading", { level: 1 })).toHaveCount(1);
      expect(await page.locator("main article").count()).toBeGreaterThan(0);
      expect(await page.locator("script").count(), "scripts on the page").toBe(0);
    });
  }
});

test.describe("SSR templates: performance budget (site/TESTING.md)", () => {
  test.skip(
    ({ viewport }) => !BEHAVIOUR_WIDTHS.includes(viewport?.width ?? 0),
    "behaviour, not layout: at 360 and 1440",
  );
  for (const path of ["/", "/economy"]) {
    test(`${path}: no shift after fonts, FCP under 1.8 s, LCP under 2.5 s`, { tag: "@smoke" }, async ({
      page,
      browserName,
    }) => {
      test.skip(browserName !== "chromium", "layout-shift and LCP entries are Chromium's");
      await gotoPage(page, path);
      const shiftSinceFonts = await watchLayoutShift(page);
      const total = await cumulativeLayoutShift(page);
      const timing = await page.evaluate(
        () =>
          new Promise<{ lcp: number; fcp: number; bytes: number }>((resolve) => {
            const fcp = performance.getEntriesByName("first-contentful-paint")[0]?.startTime ?? 0;
            const bytes = performance
              .getEntriesByType("resource")
              .concat(performance.getEntriesByType("navigation"))
              .reduce((n, e) => n + ((e as PerformanceResourceTiming).transferSize ?? 0), 0);
            new PerformanceObserver((list) => {
              const entries = list.getEntries();
              resolve({ lcp: entries.at(-1)?.startTime ?? 0, fcp, bytes });
            }).observe({ type: "largest-contentful-paint", buffered: true });
          }),
      );
      test
        .info()
        .annotations.push(
          { type: "CLS since navigation, font swap included", description: String(total) },
          { type: "timing", description: JSON.stringify(timing) },
        );
      expect(await shiftSinceFonts()).toBe(0);
      expect(timing.fcp, "first contentful paint (ms)").toBeLessThan(1800);
      expect(timing.lcp, "largest contentful paint (ms)").toBeLessThan(2500);
    });
  }
});
