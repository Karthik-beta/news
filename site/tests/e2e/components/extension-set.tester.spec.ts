// Component tester's browser checks for the #11 extension set (news-component-testing; the builder's own checks are in
// extension-set.spec.ts): SourceCard, Steps and the LineChart notes. Every fixture page, alone and in context
// (Steps/in-context: an explainer body with prose, a source card, steps and a chart with notes), on every screen of
// the matrix: reads at this size (nothing clipped, outside, overlapping or under 12px), 200% text and 200% zoom,
// fonts blocked, island script blocked, motion, forced colours, print, layout shift and JavaScript. Then each
// component's look against the tokens in both themes (system setting and data-theme), the Steps rule between the
// numbers, the note numbers on the plot (no collisions), keyboard and the accessibility tree.
import type { Locator, Page } from "@playwright/test";
import {
  cumulativeLayoutShift,
  expect,
  expectAccessible,
  expectDesignFonts,
  expectNoSidewaysScroll,
  expectNoSmallText,
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
import { expectInsideViewport, expectNoClippedText } from "./primitives";

const PAGES = [
  "SourceCard/default",
  "SourceCard/edge-cases",
  "Steps/default",
  "Steps/edge-cases",
  "Steps/in-context",
  "LineChart/notes",
] as const;
const url = (p: string) => `/fixtures/components/${p}/`;
const hasChart = (p: string) => p === "LineChart/notes" || p === "Steps/in-context";
const FLOATING = "[popover], .popover--tooltip, .chart-tooltip";
/** Projects with committed baselines, as in charts-visual.spec.ts: Chromium light and dark at 360 and 1440, light at
 * 768 and 980. */
const hasBaselines = (project: string) =>
  /^smoke-/.test(project) || /^chromium-((light|dark)-(360|1440)|light-(768|980))$/.test(project);

async function hydrateVisible(page: Page): Promise<void> {
  const islands = page.locator("[data-island]");
  for (let i = 0; i < (await islands.count()); i++) {
    const island = islands.nth(i);
    if (!(await island.isVisible())) continue;
    await island.scrollIntoViewIfNeeded();
    await expect(island.locator("[data-hydrated='true']")).toHaveCount(1);
  }
}

/** Every label a line chart with notes writes, and the notes list: none may overlap another. */
const LABELS = [
  ".line-chart__event-label",
  ".line-chart__events-label",
  ".chart-plot__x-label",
  ".chart-plot__tick",
  ".line-chart__end-label",
  ".line-chart__ends-item",
  ".chart-legend__item",
  ".line-chart__note",
  ".figure-frame__title, .chart-figure__title, figcaption h3, figure h3",
].join(", ");

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

/** The computed value of `property` for a probe styled with `value` (a token), in the page's current theme. */
const resolved = (page: Page, property: string, value: string) =>
  page.evaluate(
    ([prop, v]) => {
      const probe = document.createElement("div");
      probe.style.borderStyle = "solid";
      probe.style.setProperty(prop as string, v as string);
      document.body.append(probe);
      const out = getComputedStyle(probe).getPropertyValue(prop as string);
      probe.remove();
      return out;
    },
    [property, value],
  );

/** Checks `el`'s computed properties against tokens: [property, token value] pairs. */
async function expectTokens(page: Page, el: Locator, pairs: [string, string][]): Promise<void> {
  for (const [prop, token] of pairs) {
    const want = await resolved(page, prop, token);
    const got = await el.evaluate((node, p) => getComputedStyle(node).getPropertyValue(p), prop);
    expect(got, `${prop} = ${token}`).toBe(want);
  }
}

// ------------------------------------------------------------------------------------------------- Every page

for (const p of PAGES) {
  test(`${p}: nothing clipped, outside the screen, overlapping or under 12px`, async ({ page }) => {
    await gotoPage(page, url(p));
    await hydrateVisible(page);
    await expectNoSidewaysScroll(page);
    await expectInsideViewport(page, page.locator("main"), FLOATING);
    await expectNoClippedText(page);
    await expectNoSmallText(page);
    await expectTargetSize(page);
    await expectDesignFonts(page);
    await expectNoCollisions(page);
  });

  test(`${p}: reflows at 200% text and 200% zoom`, { tag: "@zoom" }, async ({ page }) => {
    await gotoPage(page, url(p));
    await hydrateVisible(page);
    await setTextSize(page, 200);
    await expectNoSidewaysScroll(page);
    await expectInsideViewport(page, page.locator("main"), FLOATING);
    await expectNoClippedText(page);
    await expectNoCollisions(page);
    await setTextSize(page, 100);
    test.skip(!(await zoomTo200(page)), "below 640px wide, 200% zoom is the 320 project (400%)");
    await expectNoSidewaysScroll(page);
    await expectInsideViewport(page, page.locator("main"), FLOATING);
    await expectNoClippedText(page);
    await expectNoCollisions(page);
  });

  test(`${p}: passes axe in both themes, by system setting and by data-theme`, { tag: "@a11y" }, async ({
    page,
    theme,
  }) => {
    await gotoPage(page, url(p));
    await hydrateVisible(page);
    await expectAccessible(page);
    await setTheme(page, theme === "light" ? "dark" : "light");
    await expectAccessible(page);
  });

  test.describe(() => {
    // The blocked font requests are the only failures these tests allow.
    test.use({
      allowedConsoleErrors: [[/\/fonts\//,/^Failed to load resource/, /downloadable font/i], { scope: "test" }] as never,
    });
    test(`${p}: still reads with the web fonts blocked`, async ({ page }) => {
      await page.route("**/fonts/**", (r) => r.abort());
      await page.goto(url(p));
      await expectNoSidewaysScroll(page);
      await expectInsideViewport(page, page.locator("main"), FLOATING);
      await expectNoClippedText(page);
    });
  });

  test(`${p}: JavaScript only for chart islands; no layout shift once fonts are in`, async ({ page }) => {
    const scripts: string[] = [];
    page.on("request", (r) => {
      if (r.resourceType() === "script") scripts.push(r.url());
    });
    await gotoPage(page, url(p));
    await hydrateVisible(page);
    const shift = await watchLayoutShift(page);
    if (!hasChart(p)) expect(scripts).toEqual([]);
    else expect(scripts.length).toBeGreaterThan(0);
    const total = await cumulativeLayoutShift(page);
    if (total !== null) {
      test.info().annotations.push({ type: "CLS since navigation, font swap included", description: String(total) });
      expect(await shift(), "layout shift after fonts loaded and islands hydrated").toBeLessThan(0.001);
    }
  });

  test(`${p}: nothing animates under reduced motion`, async ({ page }) => {
    await gotoPage(page, url(p));
    expect(await runningAnimations(page)).toEqual([]);
    expect(await longestMotionMs(page)).toBe(0);
  });

  test(`${p}: nothing animates on load; transitions within 200 ms`, { tag: "@motion" }, async ({ page }) => {
    await gotoPage(page, url(p));
    expect(await runningAnimations(page)).toEqual([]);
    expect(await longestMotionMs(page)).toBeLessThanOrEqual(200);
  });

  test(`${p}: keeps its content, rules and lines in forced colours, and passes axe`, { tag: "@forced-colors" }, async ({
    page,
  }) => {
    await gotoPage(page, url(p));
    await hydrateVisible(page);
    expect(await page.evaluate(() => matchMedia("(forced-colors: active)").matches)).toBe(true);
    await expectAccessible(page);
    await expectNoClippedText(page);
    const canvas = await page.evaluate(() => getComputedStyle(document.body).backgroundColor);
    // Rules and lines drawn with borders or backgrounds stay visible: not the page colour, not transparent.
    for (const [sel, prop] of [
      [".source-card:not(.source-card--missing)", "border-top-color"],
      [".steps__step:not(:last-child)", "--"],
      [".line-chart__event", "background-color"],
    ] as const) {
      for (const el of await page.locator(sel).all()) {
        if (prop === "--") {
          const c = await el.evaluate((n) => getComputedStyle(n, "::after").borderInlineStartColor);
          expect(c, `${sel}::after`).not.toBe(canvas);
          expect(c).not.toBe("rgba(0, 0, 0, 0)");
        } else {
          const c = await el.evaluate((n, pr) => getComputedStyle(n).getPropertyValue(pr), prop);
          expect(c, sel).not.toBe(canvas);
          expect(c).not.toBe("rgba(0, 0, 0, 0)");
        }
      }
    }
  });

  test(`${p}: prints every word, the notes, the steps' rule and the card's rule`, { tag: "@print" }, async ({
    page,
  }) => {
    await gotoPage(page, url(p));
    await hydrateVisible(page);
    const before = await page.locator("main").innerText();
    await page.emulateMedia({ media: "print" });
    for (const sel of [".source-card", ".steps__step", ".line-chart__notes li", ".line-chart__event-label"])
      for (const el of await page.locator(sel).all()) await expect(el).toBeVisible();
    for (const t of await page.locator(".steps__title, .steps__text, .source-card__title, .line-chart__note").all())
      expect(before).toContain((await t.innerText()).trim().split("\n")[0]);
    await expectNoSidewaysScroll(page);
    await expectNoClippedText(page);
  });

  // Baselines on the screens #71 chose (site/TESTING.md, "Screenshots"), compared in the Playwright Docker image.
  test(`${p}: matches its screenshot baseline`, { tag: ["@visual", "@smoke"] }, async ({ page }, info) => {
    test.skip(!process.env.NEWS_VISUAL, "screenshots compare only inside the Playwright Docker image");
    test.skip(!hasBaselines(info.project.name), "no baselines on this screen for this engine");
    test.skip(p === "LineChart/notes", "LC-1 open: no baseline of overlapping note numbers (SIGNOFF.md)");
    await gotoPage(page, url(p));
    await hydrateVisible(page);
    await page.evaluate(() => window.scrollTo(0, 0));
    await expect(page.locator("main")).toHaveScreenshot(`${p.replace("/", "-")}.png`, { scale: "css" });
  });
}

// ------------------------------------------------------------------------------------------------- SourceCard

test.describe("SourceCard", () => {
  test("in the tokens of this theme, by system setting and by data-theme", async ({ page, theme }) => {
    await gotoPage(page, url("SourceCard/default"));
    for (const t of [theme, theme === "light" ? "dark" : "light"] as Theme[]) {
      if (t !== theme) await setTheme(page, t);
      const card = page.locator(".source-card").first();
      await expectTokens(page, card, [
        ["border-top-color", "var(--color-ink)"],
        ["border-top-width", "var(--border-hairline)"],
        ["padding-top", "var(--space-4)"],
        ["row-gap", "var(--space-2)"],
        ["border-bottom-width", "0px"],
        ["border-left-width", "0px"],
        ["border-radius", "0px"],
        ["box-shadow", "none"],
        ["background-color", "rgba(0, 0, 0, 0)"],
      ]);
      await expectTokens(page, card.locator(".source-card__title"), [
        ["font-family", "var(--font-family-serif)"],
        ["font-weight", "var(--font-weight-semibold)"],
        ["font-size", "var(--font-size-headline-s)"],
      ]);
      await expectTokens(page, card.locator(".source-card__title a"), [["color", "var(--color-ink)"]]);
      await expectTokens(page, card.locator(".source-card__meta"), [
        ["font-family", "var(--font-family-sans)"],
        ["font-size", "var(--font-size-label)"],
        ["color", "var(--color-ink-2)"],
      ]);
      // Never crimson: the card is not emphasis.
      const brand = await resolved(page, "color", "var(--color-brand)");
      for (const el of await card.locator("*").all())
        expect(await el.evaluate((n) => getComputedStyle(n).color)).not.toBe(brand);
    }
  });

  test("the title is one link named by the document's title, with a visible focus ring", { tag: "@interaction" }, async ({
    page,
    browserName,
    isMobile,
  }) => {
    await gotoPage(page, url("SourceCard/default"));
    const link = page.locator(".source-card").getByRole("link", { name: "Apple Inc. Form 10-K, fiscal 2024", exact: true });
    await expect(link).toHaveCount(1);
    await expect(link).toHaveAttribute("href", /^https:\/\/www\.sec\.gov\//);
    test.skip(isMobile || browserName === "webkit", "WebKit does not tab to links by default; no keyboard on phones");
    await page.locator("h1").click();
    for (let i = 0; i < 40; i++) {
      await page.keyboard.press("Tab");
      if (await link.evaluate((el) => el === document.activeElement)) break;
    }
    await expect(link).toBeFocused();
    const ring = await link.evaluate((el) => {
      const s = getComputedStyle(el);
      return { style: s.outlineStyle, width: Number.parseFloat(s.outlineWidth) };
    });
    expect(ring.style).not.toBe("none");
    expect(ring.width).toBeGreaterThanOrEqual(2);
  });

  test("the accessibility tree: badges, a link named by the title, the meta line; No data says so", async ({
    page,
  }) => {
    await gotoPage(page, url("SourceCard/edge-cases"));
    const cards = page.locator(".source-card");
    await expect(cards.nth(0)).toMatchAriaSnapshot(`
      - paragraph: Primary Preprint
      - paragraph:
        - link "A test preprint on a genetic variant, fixture"
      - paragraph: Preprint, News test fixtures
    `);
    await expect(cards.nth(2)).toHaveText("No data");
    await expect(cards.nth(2).getByRole("link")).toHaveCount(0);
  });

  test("a long unbroken title wraps inside the card on every screen", async ({ page }) => {
    await gotoPage(page, url("SourceCard/edge-cases"));
    const card = page.locator(".source-card").nth(1);
    const [c, t] = await Promise.all([card.boundingBox(), card.locator(".source-card__title a").boundingBox()]);
    if (!c || !t) throw new Error("no boxes");
    expect(t.x + t.width).toBeLessThanOrEqual(c.x + c.width + 0.5);
  });

  test("the sources panel lists each card's source", async ({ page }) => {
    await gotoPage(page, url("SourceCard/default"));
    const panel = page.getByRole("region", { name: "Sources" });
    for (const title of ["Apple Inc. Form 10-K, fiscal 2024", "A test trial of a daily walking programme, fixture paper"])
      await expect(panel.getByRole("link", { name: title, exact: true })).toHaveCount(1);
  });
});

// ------------------------------------------------------------------------------------------------- Steps

test.describe("Steps", () => {
  test("in the tokens of this theme, by system setting and by data-theme", async ({ page, theme }) => {
    await gotoPage(page, url("Steps/default"));
    for (const t of [theme, theme === "light" ? "dark" : "light"] as Theme[]) {
      if (t !== theme) await setTheme(page, t);
      const step = page.locator(".steps__step").first();
      await expectTokens(page, step.locator(".steps__number"), [
        ["font-family", "var(--font-family-sans)"],
        ["font-weight", "var(--font-weight-semibold)"],
        ["font-size", "var(--font-size-headline-s)"],
        ["color", "var(--color-ink)"],
      ]);
      await expectTokens(page, step.locator(".steps__title"), [
        ["font-family", "var(--font-family-sans)"],
        ["font-weight", "var(--font-weight-semibold)"],
        ["font-size", "var(--font-size-ui)"],
        ["color", "var(--color-ink)"],
      ]);
      await expectTokens(page, step.locator(".steps__text"), [
        ["font-family", "var(--font-family-serif)"],
        ["font-size", "var(--font-size-body-s)"],
        ["color", "var(--color-ink)"],
      ]);
      const line = await step.evaluate((n) => {
        const s = getComputedStyle(n, "::after");
        return { color: s.borderInlineStartColor, width: s.borderInlineStartWidth };
      });
      expect(line.color).toBe(await resolved(page, "color", "var(--color-rule)"));
      expect(line.width).toBe(await resolved(page, "border-top-width", "var(--border-hairline)"));
      // No boxes, circles or colour.
      for (const el of await page.locator(".steps, .steps *:not([popover], [popover] *)").all()) {
        const s = await el.evaluate((n) => {
          const c = getComputedStyle(n);
          return [c.backgroundColor, c.borderRadius, c.boxShadow];
        });
        expect(s).toEqual(["rgba(0, 0, 0, 0)", "0px", "none"]);
      }
    }
  });

  /** Each step's number box, the rule under it and the next number's box, in page px. */
  const ruleGeometry = (page: Page) =>
    page.locator(".steps__list").evaluateAll((lists) =>
      lists.flatMap((list) => {
        const steps = [...list.querySelectorAll<HTMLElement>(":scope > .steps__step")];
        return steps.slice(0, -1).map((li, i) => {
          const num = (li.querySelector(".steps__number") as HTMLElement).getBoundingClientRect();
          const next = (steps[i + 1]?.querySelector(".steps__number") as HTMLElement).getBoundingClientRect();
          const body = (li.querySelector(".steps__body") as HTMLElement).getBoundingClientRect();
          const title = (li.querySelector(".steps__title") as HTMLElement).getBoundingClientRect();
          const a = getComputedStyle(li, "::after");
          const box = li.getBoundingClientRect();
          const top = box.top + Number.parseFloat(a.top);
          const bottom = box.bottom - Number.parseFloat(a.bottom);
          const left = box.left + Number.parseFloat(a.insetInlineStart || a.left);
          // The number's glyph box (a Range), not its 2em column.
          const r = document.createRange();
          r.selectNodeContents(li.querySelector(".steps__number") as Node);
          const glyph = r.getBoundingClientRect();
          return { num, next, body, title, top, bottom, left, glyph };
        });
      }),
    );

  const expectRuleBetweenNumbers = async (page: Page) => {
    const rules = await ruleGeometry(page);
    expect(rules.length).toBeGreaterThan(0);
    for (const [i, g] of rules.entries()) {
      expect(g.top, `rule ${i} starts under its number`).toBeGreaterThanOrEqual(g.glyph.bottom - 1);
      expect(g.bottom, `rule ${i} ends above the next number`).toBeLessThanOrEqual(g.next.top + 1);
      expect(g.bottom - g.top, `rule ${i} is drawn`).toBeGreaterThan(4);
      const centre = g.glyph.left + g.glyph.width / 2;
      expect(Math.abs(g.left - centre), `rule ${i} runs down the number's centre`).toBeLessThanOrEqual(1.5);
      expect(g.left, `rule ${i} stays left of the text`).toBeLessThan(g.body.left);
      // The title sits on the number's row.
      expect(Math.abs(g.title.top - g.num.top), `title ${i} on the number's row`).toBeLessThanOrEqual(1);
    }
  };

  test("the rule runs from under each number to the next, left of the text", async ({ page }) => {
    for (const p of ["Steps/default", "Steps/edge-cases"]) {
      await gotoPage(page, url(p));
      await expectRuleBetweenNumbers(page);
    }
  });

  test("the rule still joins the numbers at 200% text", { tag: "@zoom" }, async ({ page }) => {
    await gotoPage(page, url("Steps/edge-cases"));
    await setTextSize(page, 200);
    await expectRuleBetweenNumbers(page);
  });

  test("steps are 24px apart and the source line sits 16px under the list", async ({ page }) => {
    await gotoPage(page, url("Steps/default"));
    const gaps = await page.locator(".steps__step").evaluateAll((els) =>
      els.slice(1).map((el, i) => el.getBoundingClientRect().top - (els[i] as Element).getBoundingClientRect().bottom),
    );
    const s6 = Number.parseFloat(await resolved(page, "padding-top", "var(--space-6)"));
    for (const g of gaps) expect(Math.abs(g + s6) <= 1 || Math.abs(g) <= 1).toBe(true);
    const listToSource = await page
      .locator(".steps")
      .first()
      .evaluate((n) => {
        const list = (n.querySelector(".steps__list") as HTMLElement).getBoundingClientRect();
        const src = (n.querySelector(".steps__source") as HTMLElement).getBoundingClientRect();
        return src.top - list.bottom;
      });
    expect(listToSource).toBeCloseTo(Number.parseFloat(await resolved(page, "padding-top", "var(--space-4)")), 0);
    // The step itself pads 24px below all but the last.
    const pad = await page.locator(".steps__step").first().evaluate((n) => getComputedStyle(n).paddingBottom);
    expect(Number.parseFloat(pad)).toBe(s6);
  });

  test("the accessibility tree: one list of steps, the numbers left to the list, every source linked", async ({
    page,
  }) => {
    await gotoPage(page, url("Steps/default"));
    await expect(page.locator(".steps").first()).toMatchAriaSnapshot(`
      - list:
        - listitem:
          - paragraph: The committee meets
          - paragraph: /The Federal Open Market Committee meets several times a year/
        - listitem:
          - paragraph: It votes on a target range
          - paragraph: /The vote in December was/
        - listitem:
          - paragraph: It publishes a statement
          - paragraph: /The statement gives the reasons/
        - listitem:
          - paragraph: Markets move to the new range
          - paragraph: /Banks then lend to each other overnight/
      - paragraph: /Source/
    `);
  });

  test("every value in a step opens its source popover by keyboard", { tag: "@interaction" }, async ({
    page,
    isMobile,
  }) => {
    test.skip(isMobile, "no keyboard on touch screens");
    await gotoPage(page, url("Steps/default"));
    const trigger = page.locator(".steps__text button.popover-trigger").first();
    await expect(trigger).toBeVisible();
    await trigger.focus();
    await page.keyboard.press("Enter");
    const panel = page.locator("[popover]:popover-open");
    await expect(panel).toHaveCount(1);
    await page.keyboard.press("Escape");
    await expect(page.locator("[popover]:popover-open")).toHaveCount(0);
    await expect(trigger).toBeFocused();
  });
});

// ------------------------------------------------------------------------------------------------- LineChart notes

test.describe("LineChart notes", () => {
  const PATH = url("LineChart/notes");
  const figure = (page: Page, s: string) => page.locator(`figure:has([id="${s}-title"])`);

  test("note numbers on the plot never overlap each other or any other label", async ({ page }) => {
    await gotoPage(page, PATH);
    await hydrateVisible(page);
    for (const s of ["cpi", "close", "odd"]) expect(await collisions(figure(page, s)), s).toEqual([]);
  });

  test("note numbers on a tight plot (200% text) never overlap each other or any other label", { tag: "@zoom" }, async ({
    page,
  }) => {
    await gotoPage(page, PATH);
    await hydrateVisible(page);
    await setTextSize(page, 200);
    for (const s of ["cpi", "close", "odd"]) expect(await collisions(figure(page, s)), s).toEqual([]);
  });

  test("each note is a 1px ink-3 line at its period and its number is in the list under the plot", async ({
    page,
    theme,
  }) => {
    await gotoPage(page, PATH);
    await hydrateVisible(page);
    for (const t of [theme, theme === "light" ? "dark" : "light"] as Theme[]) {
      if (t !== theme) await setTheme(page, t);
      const cpi = figure(page, "cpi");
      const lines = cpi.locator(".line-chart__event");
      await expect(lines).toHaveCount(3);
      for (const l of await lines.all()) {
        await expectTokens(page, l, [["background-color", "var(--color-ink-3)"]]);
        const w = (await l.boundingBox())?.width ?? 0;
        expect(w).toBeLessThanOrEqual(1.01);
      }
      const notes = cpi.locator(".line-chart__notes");
      await expectTokens(page, notes, [
        ["font-family", "var(--font-family-sans)"],
        ["font-size", "var(--font-size-label)"],
        ["color", "var(--color-ink-2)"],
      ]);
      await expectTokens(page, notes.locator(".line-chart__note-period").first(), [
        ["font-weight", "var(--font-weight-semibold)"],
      ]);
      // Left to right: each label sits at or right of the one before.
      const xs = await lines.evaluateAll((els) => els.map((e) => e.getBoundingClientRect().left));
      expect([...xs].sort((a, b) => a - b)).toEqual(xs);
    }
  });

  test("the list sits under the plot and above the source line, 16px below the plot", async ({ page }) => {
    await gotoPage(page, PATH);
    await hydrateVisible(page);
    const cpi = figure(page, "cpi");
    const [plot, list] = await Promise.all([
      cpi.locator(".chart-plot").boundingBox(),
      cpi.locator(".line-chart__notes").boundingBox(),
    ]);
    if (!plot || !list) throw new Error("no boxes");
    expect(list.y).toBeGreaterThanOrEqual(plot.y + plot.height - 0.5);
    const source = await cpi.locator(".figure-frame__source").boundingBox();
    if (source) expect(source.y).toBeGreaterThanOrEqual(list.y + list.height - 0.5);
  });

  test("the accessibility tree: a list named Notes on the chart, period and words, numbers left to the list", async ({
    page,
  }) => {
    await gotoPage(page, PATH);
    await expect(figure(page, "cpi").locator(".line-chart__notes")).toMatchAriaSnapshot(`
      - list "Notes on the chart":
        - listitem: "Mar 2024: Spring"
        - listitem: "Sep 2024: First rate cut"
        - listitem: "Dec 2024: A longer note that reads at year end"
    `);
  });

  test("the plot keeps its keys and tooltip with notes on it", { tag: "@interaction" }, async ({ page, isMobile }) => {
    test.skip(isMobile, "no keyboard on touch screens");
    await gotoPage(page, PATH);
    await hydrateVisible(page);
    const plot = figure(page, "cpi").locator(".chart-plot__area");
    await plot.focus();
    await page.keyboard.press("End");
    await expect(page.locator(".chart-tooltip").first()).toBeVisible();
    await expect(page.locator(".chart-tooltip").first()).toContainText("Dec 2024");
    await page.keyboard.press("Escape");
    await expect(page.locator(".chart-tooltip:visible")).toHaveCount(0);
  });

  test.describe(() => {
    // Blocking the island scripts is the failure under test.
    test.use({
      allowedConsoleErrors: [
        [/^Failed to load resource/, /request failed: .*\/_astro\//, /dynamically imported/i],
        { scope: "test" },
      ] as never,
    });
    test("with the island script blocked, the numbers, lines and list still read", async ({ page }) => {
      await page.route("**/_astro/**/*.js", (r) => r.abort());
      await page.goto(PATH);
      const cpi = figure(page, "cpi");
      await expect(cpi.locator(".line-chart__event-label")).toHaveText(["1", "2", "3"]);
      await expect(cpi.locator(".line-chart__notes li")).toHaveCount(3);
      expect(await page.locator("[role='img'][tabindex]").count(), "no dead tab stop").toBe(0);
      await expectNoSidewaysScroll(page);
    });
  });
});

// ------------------------------------------------------------------------------------------------- In context

test("in context: card, steps and chart read in the explainer body, in order and apart", async ({ page }) => {
  await gotoPage(page, url("Steps/in-context"));
  await hydrateVisible(page);
  const blocks = await page
    .locator(".page-body > *")
    .evaluateAll((els) => els.map((el) => [el.className.toString(), el.getBoundingClientRect().top, el.getBoundingClientRect().bottom] as const));
  expect(blocks.length).toBeGreaterThanOrEqual(7);
  for (let i = 1; i < blocks.length; i++) {
    const gap = (blocks[i]?.[1] ?? 0) - (blocks[i - 1]?.[2] ?? 0);
    expect(gap, `space between ${blocks[i - 1]?.[0]} and ${blocks[i]?.[0]}`).toBeGreaterThanOrEqual(16);
  }
  // The card and the steps line up with the prose measure on the left.
  const lefts = await page
    .locator(".page-body > .source-card, .page-body > .steps, .page-body > .prose")
    .evaluateAll((els) => els.map((el) => Math.round(el.getBoundingClientRect().left)));
  expect(new Set(lefts).size).toBe(1);
  // Showing the card cites it: the panel lists the statement.
  await expect(page.getByRole("region", { name: "Sources" }).getByRole("link", { name: /FOMC statement|Federal Reserve/ }).first()).toBeVisible();
});
