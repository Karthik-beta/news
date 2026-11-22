// Shared checks for the primitive specs in this folder (news-component-testing). Each primitive's gallery page is
// /fixtures/primitives/<Name>/ (built with NEWS_FIXTURES=1), one <section class="fixture-case" aria-label=…> per
// case in src/components/<Name>/fixtures/cases.tsx.
import type { Locator, Page } from "@playwright/test";
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
  longestMotionMs,
  runningAnimations,
  setTextSize,
  setTheme,
  type Theme,
  test,
  watchLayoutShift,
  zoomTo200,
} from "../fixtures";

export const galleryPath = (name: string) => `/fixtures/primitives/${name}/`;

export const fixtureCase = (page: Page, name: string): Locator =>
  page.locator(`section.fixture-case[aria-label="${name.replace(/"/g, '\\"')}"]`);

/** "#F4F4F1" -> "rgb(244, 244, 241)", the form getComputedStyle returns. */
export function rgb(hex: string): string {
  const h = hex.replace("#", "");
  const [r, g, b] = [0, 2, 4].map((i) => Number.parseInt(h.slice(i, i + 2), 16));
  return `rgb(${r}, ${g}, ${b})`;
}

export const tokenRgb = (name: string, theme: Theme) => rgb(tokenValue(name, theme));

/**
 * Nothing inside `scope` reaches past the viewport's sides (clipped or pushing the page), except content inside a
 * container that scrolls on its own (wide tables) and visually hidden text.
 */
export async function expectInsideViewport(
  page: Page,
  scope: Locator = page.locator("main"),
  ignore = "",
): Promise<void> {
  const outside = await scope.evaluate((root, ignoreSel) => {
    const width = document.documentElement.clientWidth;
    const found: string[] = [];
    for (const el of root.querySelectorAll<HTMLElement>("*")) {
      if (el.closest("[popover]:not(:popover-open)")) continue;
      if (ignoreSel && el.closest(ignoreSel)) continue;
      let scroller = el.parentElement;
      let insideScroller = false;
      while (scroller && scroller !== root) {
        if (getComputedStyle(scroller).overflowX === "auto" || getComputedStyle(scroller).overflowX === "scroll") {
          insideScroller = true;
          break;
        }
        scroller = scroller.parentElement;
      }
      if (insideScroller) continue;
      const r = el.getBoundingClientRect();
      if (r.width <= 1 || r.height <= 1) continue; // visually hidden
      if (r.left < -0.5 || r.right > width + 0.5) {
        found.push(`${el.tagName.toLowerCase()}.${el.className} [${Math.round(r.left)}, ${Math.round(r.right)}]`);
      }
    }
    return found;
  }, ignore);
  expect(outside, "elements outside the viewport").toEqual([]);
}

/**
 * No element clips its own text: content wider or taller than a box that hides overflow. Text fields are left
 * out: a value longer than the field scrolls inside it, which is how every input works.
 */
export async function expectNoClippedText(page: Page, scope: Locator = page.locator("main")): Promise<void> {
  const clipped = await scope.evaluate((root) =>
    [...root.querySelectorAll<HTMLElement>("*")]
      .filter((el) => {
        if (el.matches("input, textarea")) return false;
        const s = getComputedStyle(el);
        if (!/hidden|clip/.test(s.overflowX + s.overflowY)) return false;
        if (el.getBoundingClientRect().width <= 1) return false; // visually hidden on purpose
        return el.scrollWidth > el.clientWidth + 1 || el.scrollHeight > el.clientHeight + 1;
      })
      .map((el) => `${el.tagName.toLowerCase()}.${el.className}`),
  );
  expect(clipped, "elements clipping their text").toEqual([]);
}

/**
 * The checks every primitive gallery runs on every screen, in both themes and three engines: it reads at this
 * size, passes axe in both themes (system and data-theme), survives 200% zoom and 200% text, prints, does not
 * move, keeps working with fonts blocked, and ships no JavaScript.
 */
export function galleryChecks(name: string, caseCount: number): void {
  const path = galleryPath(name);

  test(`${name} gallery: every case reads at this screen size`, async ({ page }) => {
    await gotoPage(page, path);
    await expect(page.locator(".fixture-case")).toHaveCount(caseCount);
    await expectNoSidewaysScroll(page);
    await expectInsideViewport(page);
    await expectNoClippedText(page);
    await expectNoSmallText(page);
    await expectTargetSize(page);
    await expectDesignFonts(page);
  });

  test(`${name} gallery: no WCAG 2.2 AA violations in either theme, by system setting and by data-theme`, {
    tag: "@a11y",
  }, async ({ page, theme }) => {
    await gotoPage(page, path);
    await expectAccessible(page);
    const other = theme === "light" ? "dark" : "light";
    await setTheme(page, other);
    await expectAccessible(page);
  });

  test(`${name} gallery: reflows at 200% zoom and at 200% text size`, { tag: "@zoom" }, async ({ page }) => {
    await gotoPage(page, path);
    await setTextSize(page, 200);
    await expectNoSidewaysScroll(page);
    // Tooltip fixtures sit at fixed coordinates; on a page, the island that owns a tooltip flips and clamps it.
    await expectInsideViewport(page, page.locator("main"), ".popover--tooltip");
    await expectNoClippedText(page);
    await setTextSize(page, 100);
    test.skip(!(await zoomTo200(page)), "below 640px wide, 200% zoom is the 320 project (400%)");
    await expectNoSidewaysScroll(page);
    await expectInsideViewport(page);
    await expectNoClippedText(page);
  });

  test(`${name} gallery: ships no JavaScript and does not move once the fonts are in`, async ({ page }) => {
    const scripts: string[] = [];
    page.on("request", (r) => {
      if (r.resourceType() === "script") scripts.push(r.url());
    });
    await gotoPage(page, path);
    const shift = await watchLayoutShift(page);
    expect(scripts).toEqual([]);
    expect(await page.locator("script").count()).toBe(0);
    const total = await cumulativeLayoutShift(page);
    if (total !== null) {
      test.info().annotations.push({ type: "CLS since navigation, font swap included", description: String(total) });
      expect(await shift(), "layout shift after the fonts loaded").toBeLessThan(0.001);
    }
  });

  test.describe(() => {
    // The blocked font requests are the only failures these tests allow.
    // Playwright reads an array option as [value, options], so the list goes inside a tuple.
    test.use({
      allowedConsoleErrors: [
        [/\/fonts\//, /^Failed to load resource/, /downloadable font/i],
        { scope: "test" },
      ] as never,
    });
    test(`${name} gallery: still reads with the web fonts blocked`, async ({ page }) => {
      await page.route("**/fonts/**", (r) => r.abort());
      await page.goto(path);
      await expect(page.locator(".fixture-case")).toHaveCount(caseCount);
      await expectNoSidewaysScroll(page);
      await expectInsideViewport(page);
      await expectNoClippedText(page);
    });
  });

  test(`${name} gallery: nothing animates, under reduced motion`, async ({ page }) => {
    await gotoPage(page, path);
    expect(await runningAnimations(page)).toEqual([]);
    expect(await longestMotionMs(page)).toBe(0);
  });

  test(`${name} gallery: nothing animates on load, transitions within 200 ms`, { tag: "@motion" }, async ({ page }) => {
    await gotoPage(page, path);
    expect(await runningAnimations(page)).toEqual([]);
    expect(await longestMotionMs(page)).toBeLessThanOrEqual(200);
  });

  test(`${name} gallery: keeps every case and passes axe in forced colours`, { tag: "@forced-colors" }, async ({
    page,
  }) => {
    await gotoPage(page, path);
    expect(await page.evaluate(() => matchMedia("(forced-colors: active)").matches)).toBe(true);
    await expect(page.locator(".fixture-case")).toHaveCount(caseCount);
    await expectAccessible(page);
  });

  // Baselines are made and compared only on Linux in the Playwright image: `bun run test:visual`.
  test(`${name} gallery: matches its screenshot baseline`, { tag: "@visual" }, async ({ page }) => {
    test.skip(!process.env.NEWS_VISUAL, "screenshots compare only inside the Playwright Docker image");
    await gotoPage(page, path);
    await expect(page).toHaveScreenshot(`${name}.png`, { fullPage: true, scale: "css" });
  });
}

/** The popover panel a trigger opens. */
export async function panelFor(page: Page, trigger: Locator): Promise<Locator> {
  const id = await trigger.getAttribute("popovertarget");
  if (!id) throw new Error("trigger has no popovertarget");
  return page.locator(`[id="${id}"]`);
}

/**
 * An open popover sits wholly on screen, at most 320px wide and inside the gutters (components.md), and, where the
 * engine has anchor positioning, next to its trigger (below it, or above it near the bottom edge).
 */
export async function expectPopoverPlacement(page: Page, trigger: Locator, panel: Locator): Promise<void> {
  const g = await page.evaluate(
    ([t, p]) => {
      const tr = (t as HTMLElement).getBoundingClientRect();
      const pr = (p as HTMLElement).getBoundingClientRect();
      const gutter = Number.parseFloat(getComputedStyle(document.documentElement).getPropertyValue("--gutter")) || 0;
      return {
        trigger: { top: tr.top, bottom: tr.bottom, left: tr.left, right: tr.right },
        panel: { top: pr.top, bottom: pr.bottom, left: pr.left, right: pr.right, width: pr.width },
        vw: document.documentElement.clientWidth,
        vh: window.innerHeight,
        gutter,
        anchored: CSS.supports("position-area: block-end"),
        overflowing: (p as HTMLElement).scrollWidth > (p as HTMLElement).clientWidth + 1,
      };
    },
    [await trigger.elementHandle(), await panel.elementHandle()],
  );
  test.info().annotations.push({ type: "popover geometry", description: JSON.stringify(g) });
  expect(g.panel.width, "at most 320px wide").toBeLessThanOrEqual(320.5);
  expect(g.panel.width, "inside the gutters").toBeLessThanOrEqual(g.vw - 2 * g.gutter + 0.5);
  expect(g.panel.left, "left edge on screen").toBeGreaterThanOrEqual(-0.5);
  expect(g.panel.right, "right edge on screen").toBeLessThanOrEqual(g.vw + 0.5);
  expect(g.panel.top, "top edge on screen").toBeGreaterThanOrEqual(-0.5);
  expect(g.panel.bottom, "bottom edge on screen").toBeLessThanOrEqual(g.vh + 0.5);
  expect(g.overflowing, "content wraps inside the panel").toBe(false);
  if (g.anchored) {
    const below = g.panel.top - g.trigger.bottom;
    const above = g.trigger.top - g.panel.bottom;
    expect(Math.min(Math.abs(below), Math.abs(above)), "sits right under or over its trigger").toBeLessThanOrEqual(12);
  }
}

/** Popover background and border from the tokens of `theme`. */
export async function expectPopoverLook(panel: Locator, theme: Theme): Promise<void> {
  const s = await panel.evaluate((el) => {
    const c = getComputedStyle(el);
    return {
      bg: c.backgroundColor,
      border: [c.borderTopWidth, c.borderTopStyle, c.borderTopColor],
      shadow: c.boxShadow,
      radius: c.borderTopLeftRadius,
      padding: c.paddingTop,
      family: c.fontFamily,
    };
  });
  expect(s.bg).toBe(tokenRgb("--color-paper", theme));
  expect(s.border).toEqual(["1px", "solid", tokenRgb("--color-rule", theme)]);
  expect(s.shadow).toBe("none");
  expect(s.radius).toBe("0px");
  expect(s.padding).toBe("12px");
  expect(s.family).toMatch(/Fira Sans/);
}
