// Component tester's browser checks for #11 part 1 (page types and list parts), on top of the builder's
// page-lists.spec.ts: the list parts' galleries through the shared gallery checks; every page-type fixture with its
// island hydrated, at 200% text and zoom, without motion, in forced colours and in print; the timeline's dots on its
// line at both layouts and 200% text, and its key-dot ring; the decision's facts at phone width and its rate path
// with scripts blocked; the story item's date column at 599 and 600px; the Latest time column with "14 Jan 2025";
// pagination by keyboard and touch with a visible focus ring; landmark names; crimson only in its roles; the
// dark-mode sparkline; and the findings' 24px stacking on the article fixture page.
import type { Locator, Page } from "@playwright/test";
import { tokenValue } from "../../support/tokens";
import {
  cumulativeLayoutShift,
  expect,
  expectAccessible,
  expectNoSidewaysScroll,
  expectTargetSize,
  gotoPage,
  longestMotionMs,
  runningAnimations,
  setTextSize,
  setTheme,
  type Theme,
  test,
  watchLayoutShift,
  zoomTo200,
} from "../fixtures";
import { expectInsideViewport, expectNoClippedText, galleryChecks, rgb } from "./primitives";

const FIXTURES = [
  "PolicyDecision/default",
  "PolicyDecision/edge-cases",
  "Finding/default",
  "Finding/edge-cases",
  "Timeline/default",
  "Timeline/edge-cases",
];
const PARTS: [string, number][] = [
  ["LeadStory", 6],
  ["StoryItem", 10],
  ["SectionHead", 3],
  ["LatestList", 2],
  ["PageContents", 2],
  ["RelatedPages", 3],
  ["Pagination", 5],
];
const url = (p: string) => `/fixtures/components/${p}/`;
const gallery = (n: string) => `/fixtures/primitives/${n}/`;
const FLOATING = "[popover], .popover--tooltip";
const other = (t: Theme): Theme => (t === "light" ? "dark" : "light");

async function hydrateAll(page: Page): Promise<void> {
  const islands = page.locator("[data-island]");
  for (let i = 0; i < (await islands.count()); i++) {
    await islands.nth(i).scrollIntoViewIfNeeded();
    await expect(islands.nth(i).locator("[data-hydrated='true']")).toHaveCount(1);
  }
}

for (const [name, cases] of PARTS) galleryChecks(name, cases);

for (const p of FIXTURES) {
  test(`${p}: nothing clipped or off screen once the island has hydrated`, async ({ page }) => {
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
    await expectNoClippedText(page);
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

  test(`${p}: passes axe in forced colours`, { tag: "@forced-colors" }, async ({ page }) => {
    await gotoPage(page, url(p));
    await hydrateAll(page);
    expect(await page.evaluate(() => matchMedia("(forced-colors: active)").matches)).toBe(true);
    await expectAccessible(page);
  });

  test(`${p}: prints every word, without the interface`, { tag: "@print" }, async ({ page }) => {
    await gotoPage(page, url(p));
    await hydrateAll(page);
    await page.emulateMedia({ media: "print" });
    for (const el of await page.locator(".policy-decision, .finding, .timeline").all()) await expect(el).toBeVisible();
    for (const actions of await page.locator(".figure-frame__actions, .data-links").all()) {
      await expect(actions).toBeHidden();
    }
    // Timeline dots are drawn with print-color-adjust: exact, so they survive "background graphics off".
    for (const dot of await page.locator(".timeline__event").all()) {
      expect(await dot.evaluate((li) => getComputedStyle(li, "::after").printColorAdjust)).toBe("exact");
    }
  });
}

// ---------------------------------------------------------------------------------------------------- Timeline

interface EventGeometry {
  dotX: number;
  dotY: number;
  dotLeft: number;
  dotRight: number;
  lineX: number;
  lineTop: number;
  lineBottom: number;
  textY: number | null;
  dateLeft: number;
  dateRight: number;
  textLeft: number;
  key: boolean;
  outline: string;
  outlineOffset: string;
  dotColour: string;
}

/** Each event's dot, its stretch of line and the first visible line of its date, in page coordinates. */
const timelineGeometry = (list: Locator) =>
  list.locator(":scope > .timeline__event").evaluateAll((events) =>
    events.map((li): EventGeometry => {
      const r = li.getBoundingClientRect();
      const dot = getComputedStyle(li, "::after");
      const line = getComputedStyle(li, "::before");
      const px = (v: string) => Number.parseFloat(v);
      const date = li.querySelector(".timeline__date") as HTMLElement;
      const range = document.createRange();
      range.selectNodeContents(date);
      // Measure the date itself: the visually hidden "Key event: " is clipped, but its text still has full-size rects.
      const hidden = date.querySelector(".visually-hidden");
      if (hidden) range.setStartAfter(hidden);
      const first = [...range.getClientRects()].find((b) => b.width > 2 && b.height > 2);
      const lineTop = r.top + (line.content === "none" ? 0 : px(line.top));
      const lineHeight = line.content === "none" ? 0 : px(line.height);
      const text = li.querySelector(".timeline__text") as HTMLElement;
      return {
        dotX: r.left + px(dot.left) + px(dot.width) / 2,
        dotY: r.top + px(dot.top) + px(dot.height) / 2,
        dotLeft: r.left + px(dot.left),
        dotRight: r.left + px(dot.left) + px(dot.width),
        lineX: r.left + px(line.left) + px(line.borderLeftWidth) / 2,
        lineTop,
        lineBottom: lineTop + lineHeight,
        textY: first ? (first.top + first.bottom) / 2 : null,
        dateLeft: date.getBoundingClientRect().left,
        dateRight: date.getBoundingClientRect().right,
        textLeft: text.getBoundingClientRect().left,
        key: li.classList.contains("timeline__event--key"),
        outline: `${dot.outlineStyle} ${dot.outlineWidth} ${dot.outlineColor}`,
        outlineOffset: dot.outlineOffset,
        dotColour: dot.backgroundColor,
      };
    }),
  );

function expectOnTheLine(events: EventGeometry[], where: string): void {
  expect(events.length).toBeGreaterThan(0);
  events.forEach((e, i) => {
    const at = `${where}, event ${i + 1}`;
    if (events.length > 1)
      expect(Math.abs(e.lineX - e.dotX), `${at}: dot centred on the line`).toBeLessThanOrEqual(0.75);
    expect(e.textY, `${at}: date has a line box`).not.toBeNull();
    expect(Math.abs((e.textY ?? 0) - e.dotY), `${at}: dot level with the date's first line`).toBeLessThanOrEqual(1.5);
    // The ring is 1px, 2px out: it must clear the text too. The date sits after the dot on phones, before it from 600px.
    expect(e.textLeft - (e.dotRight + 3), `${at}: text clear of the dot and its ring`).toBeGreaterThanOrEqual(0);
    const dateClear = e.dateLeft >= e.dotRight + 3 || e.dateRight <= e.dotLeft - 3;
    expect(dateClear, `${at}: date clear of the dot and its ring`).toBe(true);
  });
  if (events.length < 2) return;
  const first = events[0] as EventGeometry;
  const last = events.at(-1) as EventGeometry;
  expect(Math.abs(first.lineTop - first.dotY), `${where}: the line starts at the first dot`).toBeLessThanOrEqual(4.5);
  expect(Math.abs(last.lineBottom - last.dotY), `${where}: the line ends at the last dot`).toBeLessThanOrEqual(4.5);
  for (let i = 1; i < events.length; i++) {
    const gap = (events[i] as EventGeometry).lineTop - (events[i - 1] as EventGeometry).lineBottom;
    expect(Math.abs(gap), `${where}: no gap in the line before event ${i + 1}`).toBeLessThanOrEqual(0.5);
  }
}

test.describe("Timeline: dots on the line, level with the date, at every text size", () => {
  test("default and edge cases at 100% and 200% text, and at 200% zoom", { tag: "@smoke" }, async ({ page }) => {
    for (const fixture of ["default", "edge-cases"]) {
      await gotoPage(page, url(`Timeline/${fixture}`));
      for (const size of [100, 200]) {
        await setTextSize(page, size);
        const lists = page.locator("ol.timeline");
        for (let i = 0; i < (await lists.count()); i++) {
          expectOnTheLine(await timelineGeometry(lists.nth(i)), `${fixture} list ${i + 1} at ${size}% text`);
        }
      }
      await setTextSize(page, 100);
    }
    if (await zoomTo200(page)) {
      await gotoPage(page, url("Timeline/default"));
      expectOnTheLine(await timelineGeometry(page.locator("ol.timeline")), "default at 200% zoom");
    }
  });

  test("the date is above the text on phones and in a 7em column from 600px", async ({ page }) => {
    await gotoPage(page, url("Timeline/default"));
    const layout = () =>
      page
        .locator(".timeline__event")
        .first()
        .evaluate((li) => {
          const d = (li.querySelector(".timeline__date") as HTMLElement).getBoundingClientRect();
          const b = (li.querySelector(".timeline__body") as HTMLElement).getBoundingClientRect();
          const em = Number.parseFloat(getComputedStyle(li.querySelector(".timeline__date") as HTMLElement).fontSize);
          return {
            above: d.bottom <= b.top + 0.5,
            beside: d.right <= b.left && Math.abs(d.top - b.top) < 1,
            w: d.width,
            em,
          };
        });
    const width = page.viewportSize()?.width ?? 0;
    const l = await layout();
    if (width < 600) expect(l.above, "date above the text").toBe(true);
    else {
      expect(l.beside, "date in its own column").toBe(true);
      expect(Math.abs(l.w - 7 * l.em)).toBeLessThanOrEqual(1);
    }
  });

  test("the key dot is crimson with a 1px ring 2px out in both themes; every other dot is ink", async ({
    page,
    theme,
  }) => {
    await gotoPage(page, url("Timeline/default"));
    for (const t of [theme, other(theme)]) {
      await setTheme(page, t);
      const events = await timelineGeometry(page.locator("ol.timeline"));
      const brand = rgb(tokenValue("--color-brand", t));
      const ink = rgb(tokenValue("--color-ink", t));
      expect(events.filter((e) => e.key)).toHaveLength(1);
      for (const e of events) {
        if (e.key) {
          expect(e.dotColour, t).toBe(brand);
          expect(e.outline, t).toBe(`solid 1px ${brand}`);
          expect(e.outlineOffset, t).toBe("2px");
        } else {
          expect(e.dotColour, t).toBe(ink);
          expect(e.outline.startsWith("none"), t).toBe(true);
        }
      }
    }
  });

  test("forced colours: dots drawn in system colours, the key dot still ringed and distinct", {
    tag: "@forced-colors",
  }, async ({ page }) => {
    await gotoPage(page, url("Timeline/default"));
    const events = await timelineGeometry(page.locator("ol.timeline"));
    const canvas = await page.evaluate(() => getComputedStyle(document.body).backgroundColor);
    const key = events.find((e) => e.key) as EventGeometry;
    const plain = events.filter((e) => !e.key);
    expect(key.outline.startsWith("solid 1px"), "ring kept").toBe(true);
    expect(key.dotColour).not.toBe(canvas);
    for (const e of plain) {
      expect(e.dotColour).not.toBe(canvas);
      expect(e.dotColour).not.toBe("rgba(0, 0, 0, 0)");
      expect(e.dotColour).not.toBe(key.dotColour);
    }
    expectOnTheLine(events, "forced colours");
  });
});

// ------------------------------------------------------------------------------------------------ PolicyDecision

test.describe("PolicyDecision", () => {
  test("facts: one row of up to three from 600px, two across on phones, a quoted stance across the row", async ({
    page,
  }) => {
    await gotoPage(page, url("PolicyDecision/edge-cases"));
    await setTextSize(page, 100);
    const width = page.viewportSize()?.width ?? 0;
    const rows = await page.locator(".policy-decision__facts").evaluateAll((lists) =>
      lists.map((l) => ({
        width: l.getBoundingClientRect().width,
        facts: [...l.querySelectorAll(".policy-decision__fact")].map((f) => {
          const r = f.getBoundingClientRect();
          return { w: r.width, top: r.top, q: f.classList.contains("policy-decision__fact--q") };
        }),
        overflow: l.scrollWidth > l.clientWidth + 1,
      })),
    );
    expect(rows.length).toBe(4);
    for (const row of rows) {
      expect(row.overflow).toBe(false);
      for (const f of row.facts) {
        if (f.q && width < 600) expect(Math.abs(f.w - row.width), "quoted stance spans the row").toBeLessThanOrEqual(1);
        else if (row.facts.length > 1) expect(f.w).toBeLessThan(row.width * 0.6);
      }
    }
    // The stance row: on phones the quote sits above the vote; from 600px they sit side by side.
    const stance = rows[3]?.facts ?? [];
    if (width < 600) expect((stance[1]?.top ?? 0) > (stance[0]?.top ?? 0)).toBe(true);
    else expect(Math.abs((stance[1]?.top ?? 0) - (stance[0]?.top ?? 0))).toBeLessThanOrEqual(1);
  });

  test("the rate path hydrates inside the decision with no warnings, and reads with its script blocked", async ({
    page,
  }) => {
    await gotoPage(page, url("PolicyDecision/default"));
    await hydrateAll(page);
    const path = page.locator(".policy-decision__path figure");
    await expect(path).toHaveCount(1);
    await expect(path.locator("svg").first()).toBeVisible();
    // The rate path's source line is its own; the decision's sits above it, once.
    await expect(page.locator(".policy-decision > .policy-decision__source")).toHaveCount(1);
    expect(await page.evaluate(() => document.querySelectorAll(".policy-decision").length)).toBe(1);
  });

  test.describe(() => {
    test.use({
      allowedConsoleErrors: [
        [/^Failed to load resource/, /request failed: .*\/_astro\//, /dynamically imported/i],
        { scope: "test" },
      ] as never,
    });
    test("with island scripts blocked the decision and its rate path still read", async ({ page }) => {
      await page.route("**/_astro/**/*.js", (r) => r.abort());
      await page.goto(url("PolicyDecision/default"));
      await expect(page.locator(".policy-decision__rate .source-ref").first()).toHaveText("4.50%");
      await expect(page.locator(".policy-decision__change")).toHaveText("Cut by 0.25 points");
      const fig = page.locator(".policy-decision__path figure");
      await expect(fig.locator("svg").first()).toBeVisible();
      await fig.getByText("Show the data table").click();
      await expect(fig.locator("tbody tr").first()).toBeVisible();
      await expectNoSidewaysScroll(page);
    });
  });
});

// ------------------------------------------------------------------------------------------------------- Lists

test("StoryItem: the hub and search date column sits above the item below 600px and beside it from 600px", async ({
  page,
}) => {
  const size = page.viewportSize() ?? { width: 1440, height: 900 };
  for (const width of [599, 600]) {
    await page.setViewportSize({ width, height: size.height });
    await gotoPage(page, gallery("StoryItem"));
    for (const text of [100, 200]) {
      await setTextSize(page, text);
      const items = await page.locator(".story-item--hub, .story-item--search").evaluateAll((els) =>
        els.map((it) => {
          const d = (it.querySelector(".story-item__date") as HTMLElement).getBoundingClientRect();
          const b = (it.querySelector(".story-item__body") as HTMLElement).getBoundingClientRect();
          const time = it.querySelector("time") as HTMLElement;
          const range = document.createRange();
          range.selectNodeContents(time);
          const t = range.getBoundingClientRect();
          return { d, b, textRight: t.right, right: document.documentElement.clientWidth };
        }),
      );
      expect(items).toHaveLength(7);
      for (const { d, b, textRight, right } of items) {
        const at = `${width}px, ${text}% text`;
        if (width < 600) expect(d.bottom, `${at}: date above`).toBeLessThanOrEqual(b.top + 0.5);
        else {
          expect(d.right, `${at}: date beside`).toBeLessThanOrEqual(b.left);
          expect(textRight, `${at}: the date fits its column`).toBeLessThanOrEqual(d.right + 0.5);
        }
        expect(b.right, `${at}: long headlines wrap inside the page`).toBeLessThanOrEqual(right + 0.5);
      }
    }
  }
});

test('LatestList: the time column holds "14 Jan 2025" at 100% and 200% text without touching the headline', async ({
  page,
}) => {
  await gotoPage(page, gallery("LatestList"));
  for (const text of [100, 200]) {
    await setTextSize(page, text);
    const rows = await page.locator(".latest-list__item").evaluateAll((items) =>
      items.map((li) => {
        const time = li.querySelector("time") as HTMLElement;
        const link = li.querySelector("a") as HTMLElement;
        const range = document.createRange();
        range.selectNodeContents(time);
        const t = range.getBoundingClientRect();
        return { text: time.textContent, textRight: t.right, linkLeft: link.getBoundingClientRect().left, top: t.top };
      }),
    );
    expect(rows.map((r) => r.text)).toContain("14 Jan 2025");
    for (const r of rows) {
      expect(r.textRight, `${r.text} at ${text}%`).toBeLessThanOrEqual(r.linkLeft - 4);
    }
  }
  await setTextSize(page, 100);
  await expectNoSidewaysScroll(page);
});

test("crimson only in its roles: fly titles, the key dot, the current page and the brand rule", async ({
  page,
  theme,
}) => {
  // The fixture page's own "Component fixture" / "Primitive" fly line is the page shell's (.text-fly).
  const ALLOWED = ".fly-title, .fly-title *, .text-fly, .latest-list__fly";
  for (const path of [...FIXTURES.map(url), ...PARTS.map(([n]) => gallery(n))]) {
    await gotoPage(page, path);
    for (const t of [theme, other(theme)]) {
      await setTheme(page, t);
      const brand = rgb(tokenValue("--color-brand", t));
      const misused = await page.locator("main").evaluate(
        (main, { brand, allowed }) =>
          [...main.querySelectorAll<HTMLElement>("*")]
            .filter((el) => {
              const s = getComputedStyle(el);
              const text = [...el.childNodes].some((n) => n.nodeType === Node.TEXT_NODE && n.textContent?.trim());
              return text && s.color === brand && !el.matches(allowed);
            })
            .map((el) => `${el.tagName.toLowerCase()}.${el.className}: ${el.textContent?.trim().slice(0, 30)}`),
        { brand, allowed: ALLOWED },
      );
      expect(misused, `${path} ${t}: crimson text outside its roles`).toEqual([]);
      // No change, figure or decision word is ever crimson.
      const figures = await page
        .locator(
          ".policy-decision__change, .policy-decision__rate, .finding__value, .slot-figure__figure, .story-item__meta",
        )
        .evaluateAll((els) => els.map((el) => getComputedStyle(el).color));
      expect(figures, `${path} ${t}`).not.toContain(brand);
    }
  }
});

test("dark-mode sparklines: the comparison line and highlight dot use the dark tokens, with a paper ring", async ({
  page,
}) => {
  for (const path of [gallery("StoryItem"), gallery("LeadStory")]) {
    await gotoPage(page, path);
    await setTheme(page, "dark");
    const sparks = await page.locator("svg.sparkline").evaluateAll((svgs) =>
      svgs.map((svg) => ({
        line: getComputedStyle(svg.querySelector(".sparkline__line") as Element).stroke,
        dot: getComputedStyle(svg.querySelector(".sparkline__dot") as Element).fill,
        ring: getComputedStyle(svg.querySelector(".sparkline__dot") as Element).stroke,
        name: svg.getAttribute("aria-label"),
      })),
    );
    expect(sparks.length).toBeGreaterThanOrEqual(2);
    for (const s of sparks) {
      expect(s.line).toBe(rgb(tokenValue("--chart-comparison", "dark")));
      expect(s.dot).toBe(rgb(tokenValue("--chart-highlight", "dark")));
      expect(s.ring).toBe(rgb(tokenValue("--color-paper", "dark")));
      expect(s.name).toMatch(/^Trend, .+: .+ to .+$/);
    }
  }
});

test("landmark names: every landmark on each page is named, and no two share a name (LM-1)", async ({ page }) => {
  for (const [name] of PARTS) {
    await gotoPage(page, gallery(name));
    const landmarks = await page.evaluate(() => {
      const nameOf = (el: Element) => {
        const by = el.getAttribute("aria-labelledby");
        if (by) return document.getElementById(by)?.textContent?.trim() ?? "";
        return el.getAttribute("aria-label") ?? "";
      };
      return [...document.querySelectorAll("nav, section[aria-labelledby], section[aria-label], aside")]
        .filter((el) => !el.classList.contains("fixture-case"))
        .map((el) => ({ role: el.tagName.toLowerCase(), name: nameOf(el), part: el.className.split(" ")[0] ?? "" }));
    });
    for (const l of landmarks) expect(l.name, `${name}: ${l.part} landmark has a name`).not.toBe("");
    const keys = landmarks.map((l) => `${l.role} "${l.name}"`);
    expect(
      keys.filter((k, i) => keys.indexOf(k) !== i),
      `${name}: repeated landmark names`,
    ).toEqual([]);
    if (name === "Pagination") {
      // Several paginations on one page must each be named: the gallery's four are.
      expect(landmarks.filter((l) => l.role === "nav").map((l) => l.name)).toEqual([
        "Pages, first-page case",
        "Pages, long-run case",
        "Pages, last-page case",
        "Pages, near-start case",
      ]);
    }
  }
});

// -------------------------------------------------------------------------------------------------- Pagination

test.describe("Pagination: keyboard and touch", () => {
  test("Tab shows the focus ring on each link; Enter follows it to the right page", {
    tag: ["@interaction", "@smoke"],
  }, async ({ page, theme, browserName }) => {
    await gotoPage(page, gallery("Pagination"));
    const nav = page.getByRole("navigation", { name: "Pages, long-run case" });
    const links = nav.getByRole("link");
    await links.first().focus();
    // Focus from the keyboard: move back and forth so :focus-visible applies.
    await page.keyboard.press("Shift+Tab");
    await page.keyboard.press("Tab");
    const focus = rgb(tokenValue("--color-focus", theme));
    for (let i = 0; i < 7; i++) {
      // Playwright's WebKit on Windows never moves to links with Tab (Safari's default, as in SourceRef.spec.ts),
      // so there each link is focused directly, after a key press, to check how its focus shows.
      if (browserName === "webkit") await links.nth(i).focus();
      const ring = await page.evaluate(() => {
        const el = document.activeElement as HTMLElement;
        const s = getComputedStyle(el);
        return { text: el.textContent, style: s.outlineStyle, width: s.outlineWidth, colour: s.outlineColor };
      });
      expect(ring.style, `${ring.text}: focus ring`).toBe("solid");
      expect(Number.parseFloat(ring.width)).toBeGreaterThanOrEqual(2);
      expect(ring.colour).toBe(focus);
      await page.keyboard.press("Tab");
    }
    await nav.getByRole("link", { name: "Page 10" }).focus();
    await page.keyboard.press("Enter");
    await expect(page).toHaveURL(/[?&]pagination=middle&page=10$/);
  });

  test("touch: every link is at least 24px square and a tap follows it", { tag: "@interaction" }, async ({
    page,
    hasTouch,
  }) => {
    test.skip(!hasTouch, "touch screens only");
    await gotoPage(page, gallery("Pagination"));
    await expectTargetSize(page);
    const sizes = await page.locator(".pagination a").evaluateAll((as) =>
      as.map((a) => {
        const r = a.getBoundingClientRect();
        return Math.min(r.width, r.height);
      }),
    );
    expect(Math.min(...sizes)).toBeGreaterThanOrEqual(24);
    await page.getByRole("navigation", { name: "Pages, last-page case" }).getByRole("link", { name: "Newer" }).tap();
    await expect(page).toHaveURL(/[?&]pagination=last&page=23$/);
  });

  test("the current page is marked by weight and a 2px crimson underline, not by colour alone", async ({
    page,
    theme,
  }) => {
    await gotoPage(page, gallery("Pagination"));
    for (const t of [theme, other(theme)]) {
      await setTheme(page, t);
      const current = page.locator('.pagination a[aria-current="page"]').first();
      const s = await current.evaluate((a) => {
        const c = getComputedStyle(a);
        return { weight: c.fontWeight, border: `${c.borderBottomWidth} ${c.borderBottomStyle} ${c.borderBottomColor}` };
      });
      expect(Number(s.weight)).toBeGreaterThanOrEqual(600);
      expect(s.border).toBe(`2px solid ${rgb(tokenValue("--color-brand", t))}`);
    }
  });
});

// --------------------------------------------------------------------------------------------------- In context

test("in context: findings stack 24px apart in the article body, and every block clears the next", async ({ page }) => {
  await gotoPage(page, url("Finding/default"));
  const gaps = await page
    .locator(".finding")
    .evaluateAll((els) =>
      els.slice(1).map((el, i) => el.getBoundingClientRect().top - (els[i] as Element).getBoundingClientRect().bottom),
    );
  expect(gaps.length).toBeGreaterThan(0);
  for (const g of gaps) expect(Math.abs(g - 24)).toBeLessThanOrEqual(1);
  for (const p of FIXTURES) {
    await gotoPage(page, url(p));
    await hydrateAll(page);
    const blocks = await page
      .locator(".page-body > *")
      .evaluateAll((els) => els.map((el) => el.getBoundingClientRect()).map((r) => ({ top: r.top, bottom: r.bottom })));
    for (let i = 1; i < blocks.length; i++) {
      const gap = (blocks[i]?.top ?? 0) - (blocks[i - 1]?.bottom ?? 0);
      expect(gap, `${p}: space between block ${i} and ${i + 1}`).toBeGreaterThanOrEqual(16);
    }
  }
});
