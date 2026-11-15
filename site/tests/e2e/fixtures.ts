// Shared Playwright fixtures and checks for every page, fixture page and component test.
// Import `test` and `expect` from here, never from @playwright/test, so the console guard always runs.
import AxeBuilder from "@axe-core/playwright";
import { test as base, expect, type Locator, type Page } from "@playwright/test";
import { tokenValue } from "../support/tokens";

export type Theme = "light" | "dark";

export interface NewsOptions {
  /** The project's system colour scheme; tests compare against it. Set per project in playwright.config.ts. */
  theme: Theme;
  /** Console errors a test expects (for example a deliberately blocked image). Anything else fails the test. */
  allowedConsoleErrors: RegExp[];
}

/** WCAG 2.2 A and AA rule tags: the target in design/accessibility.md. */
export const WCAG_TAGS = ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa"];

export const test = base.extend<NewsOptions & { consoleGuard: undefined }>({
  theme: ["light", { option: true }],
  allowedConsoleErrors: [[], { option: true }],

  // Auto fixture: fails the test on console errors, uncaught exceptions and failed same-origin requests.
  consoleGuard: [
    async ({ page, allowedConsoleErrors, baseURL }, use) => {
      const problems: string[] = [];
      const allowed = (text: string) => allowedConsoleErrors.some((re) => re.test(text));
      page.on("console", (msg) => {
        if (msg.type() === "error" && !allowed(msg.text())) problems.push(`console.error: ${msg.text()}`);
      });
      page.on("pageerror", (err) => {
        if (!allowed(err.message)) problems.push(`uncaught: ${err.message}`);
      });
      page.on("response", (res) => {
        const text = `HTTP ${res.status()} ${res.url()}`;
        if (res.status() >= 400 && baseURL && res.url().startsWith(baseURL) && !allowed(text)) problems.push(text);
      });
      page.on("requestfailed", (req) => {
        const text = `request failed: ${req.url()} (${req.failure()?.errorText})`;
        if (baseURL && req.url().startsWith(baseURL) && !allowed(text)) problems.push(text);
      });
      await use(undefined);
      expect(problems, "console errors, uncaught exceptions or failed requests").toEqual([]);
    },
    { auto: true },
  ],
});

export { expect };

/** Navigates and waits until fonts are ready, so measurements and screenshots are stable. */
export async function gotoPage(page: Page, path: string): Promise<void> {
  await page.goto(path);
  await page.evaluate(() => document.fonts.ready.then(() => undefined));
}

/**
 * axe-core with the WCAG 2.2 AA rules. Fails on any violation (not only serious and critical), and attaches
 * the full result to the report. Scope with `include` to test one component inside a fixture page.
 */
export async function expectAccessible(
  page: Page,
  options: { include?: string; exclude?: string[]; disableRules?: string[] } = {},
): Promise<void> {
  let builder = new AxeBuilder({ page }).withTags(WCAG_TAGS);
  if (options.include) builder = builder.include(options.include);
  for (const sel of options.exclude ?? []) builder = builder.exclude(sel);
  if (options.disableRules?.length) builder = builder.disableRules(options.disableRules);
  const results = await builder.analyze();
  const summary = results.violations.map((v) => ({
    rule: v.id,
    impact: v.impact,
    help: v.help,
    targets: v.nodes.map((n) => n.target.join(" ")),
  }));
  if (summary.length > 0) {
    await test.info().attach("axe-violations.json", {
      body: JSON.stringify(results.violations, null, 2),
      contentType: "application/json",
    });
  }
  expect(summary, "axe WCAG 2.2 AA violations").toEqual([]);
}

/** No sideways page scroll (design/accessibility.md: reflow at 320px without horizontal scrolling). */
export async function expectNoSidewaysScroll(page: Page): Promise<void> {
  const { scrollWidth, clientWidth } = await page.evaluate(() => ({
    scrollWidth: document.documentElement.scrollWidth,
    clientWidth: document.documentElement.clientWidth,
  }));
  expect(scrollWidth, `page is ${scrollWidth}px wide in a ${clientWidth}px viewport`).toBeLessThanOrEqual(clientWidth);
}

/** No visible text smaller than `min` CSS px (12px is the floor at every screen size). */
export async function expectNoSmallText(page: Page, min = 12, scope: Locator = page.locator("body")): Promise<void> {
  const small = await scope.evaluate((root, minPx) => {
    const found: string[] = [];
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
    for (let node = walker.nextNode(); node; node = walker.nextNode()) {
      const el = node.parentElement;
      if (!el || !node.textContent?.trim()) continue;
      const rect = el.getBoundingClientRect();
      const style = getComputedStyle(el);
      const visible = rect.width > 0 && rect.height > 0 && style.visibility !== "hidden";
      const srOnly = rect.width <= 1 || rect.height <= 1; // visually hidden text is for screen readers
      if (visible && !srOnly && Number.parseFloat(style.fontSize) < minPx) {
        found.push(`${el.tagName.toLowerCase()} ${style.fontSize}: "${node.textContent.trim().slice(0, 40)}"`);
      }
    }
    return found;
  }, min);
  expect(small, `text under ${min}px`).toEqual([]);
}

/**
 * Every interactive target is at least `min` x `min` CSS px (WCAG 2.2 2.5.8). Inline links inside a sentence
 * are exempt by the standard, so elements laid out `display: inline` are skipped.
 */
export async function expectTargetSize(page: Page, min = 24): Promise<void> {
  const small = await page.evaluate((minPx) => {
    const selector =
      'a[href], button, input:not([type="hidden"]), select, textarea, summary, [role="button"], [role="link"], [tabindex]:not([tabindex="-1"])';
    return [...document.querySelectorAll<HTMLElement>(selector)]
      .filter((el) => getComputedStyle(el).display !== "inline")
      .map((el) => ({ el, rect: el.getBoundingClientRect() }))
      .filter(({ rect }) => rect.width > 0 && rect.height > 0 && (rect.width < minPx || rect.height < minPx))
      .map(({ el, rect }) => `${el.tagName.toLowerCase()} ${Math.round(rect.width)}x${Math.round(rect.height)}`);
  }, min);
  expect(small, `targets under ${min}x${min}px`).toEqual([]);
}

/** Forces a theme through `data-theme` (what the site's switch does), or returns to the system setting. */
export async function setTheme(page: Page, theme: Theme | "system"): Promise<void> {
  await page.evaluate((t) => {
    if (t === "system") document.documentElement.removeAttribute("data-theme");
    else document.documentElement.setAttribute("data-theme", t);
  }, theme);
}

/** The page shows `theme`: the paper and ink tokens resolve to that theme's values from tokens.json. */
export async function expectTheme(page: Page, theme: Theme): Promise<void> {
  const actual = await page.evaluate(() => {
    const css = getComputedStyle(document.documentElement);
    return {
      paper: css.getPropertyValue("--color-paper").trim().toLowerCase(),
      ink: css.getPropertyValue("--color-ink").trim().toLowerCase(),
      scheme: css.colorScheme,
    };
  });
  expect(actual).toEqual({
    paper: tokenValue("--color-paper", theme).toLowerCase(),
    ink: tokenValue("--color-ink", theme).toLowerCase(),
    scheme: theme,
  });
}

/**
 * Every visible piece of text is set in one of the two design typefaces (design/typography.md: Literata and
 * Fira Sans, no third face, no monospace): its computed font-family list is exactly one of the font tokens.
 */
export async function expectDesignFonts(page: Page, scope: Locator = page.locator("body")): Promise<void> {
  const allowed = ["--font-family-serif", "--font-family-sans"].map((t) => squashFont(tokenValue(t)));
  const fonts = await scope.evaluate((root) => {
    const found: { family: string; text: string; tag: string }[] = [];
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
    for (let node = walker.nextNode(); node; node = walker.nextNode()) {
      const el = node.parentElement;
      if (!el || !node.textContent?.trim()) continue;
      const rect = el.getBoundingClientRect();
      if (rect.width === 0 || rect.height === 0) continue;
      found.push({
        family: getComputedStyle(el).fontFamily,
        text: node.textContent.trim().slice(0, 40),
        tag: el.tagName.toLowerCase(),
      });
    }
    return found;
  });
  const wrong = fonts
    .filter((f) => !allowed.includes(squashFont(f.family)))
    .map((f) => `${f.tag} [${f.family}]: "${f.text}"`);
  expect(wrong, "text outside the two design typefaces").toEqual([]);
}

const squashFont = (family: string) =>
  family
    .replace(/["']/g, "")
    .replace(/\s*,\s*/g, ",")
    .trim()
    .toLowerCase();

/**
 * Cumulative layout shift since navigation (the page must not move: CLS 0). Only Chromium exposes layout-shift
 * entries; returns null elsewhere, so callers skip the assertion in Firefox and WebKit.
 */
export async function cumulativeLayoutShift(page: Page): Promise<number | null> {
  return page.evaluate(
    () =>
      new Promise<number | null>((resolve) => {
        if (!PerformanceObserver.supportedEntryTypes?.includes("layout-shift")) {
          resolve(null);
          return;
        }
        type Shift = PerformanceEntry & { value: number; hadRecentInput: boolean };
        let cls = 0;
        const add = (entries: PerformanceEntry[]) => {
          for (const e of entries as Shift[]) if (!e.hadRecentInput) cls += e.value;
        };
        const observer = new PerformanceObserver((list) => add(list.getEntries()));
        observer.observe({ type: "layout-shift", buffered: true });
        setTimeout(() => {
          add(observer.takeRecords());
          observer.disconnect();
          resolve(cls);
        }, 250);
      }),
  );
}

/**
 * Emulates 200% browser zoom: the CSS viewport becomes half as wide and half as tall (a 1280px window shows a
 * 640px layout). Returns false when that would go below 320px, which the 320 project already covers (400%).
 */
export async function zoomTo200(page: Page): Promise<boolean> {
  const size = page.viewportSize();
  if (!size || size.width / 2 < 320) return false;
  await page.setViewportSize({ width: Math.floor(size.width / 2), height: Math.floor(size.height / 2) });
  return true;
}

/** Text size at `percent`% of the default (the "200% text size" check), without zooming the layout. */
export async function setTextSize(page: Page, percent: number): Promise<void> {
  await page.evaluate((p) => {
    document.documentElement.style.fontSize = `${p}%`;
  }, percent);
}

/** Longest transition or animation duration on the page, in ms (motion must stay within 200ms). */
export async function longestMotionMs(page: Page): Promise<number> {
  return page.evaluate(() => {
    const ms = (v: string) =>
      Math.max(0, ...v.split(",").map((s) => (s.trim().endsWith("ms") ? 1 : 1000) * Number.parseFloat(s)));
    let longest = 0;
    for (const el of document.querySelectorAll("*")) {
      const s = getComputedStyle(el);
      longest = Math.max(longest, ms(s.transitionDuration), ms(s.animationDuration));
    }
    return longest;
  });
}

/** Animations running right now (nothing may animate on load). */
export async function runningAnimations(page: Page): Promise<string[]> {
  return page.evaluate(() =>
    document
      .getAnimations()
      .filter((a) => a.playState === "running")
      .map((a) => (a as CSSAnimation).animationName ?? a.constructor.name),
  );
}
