// Tester's browser checks for the #7 family (news-component-testing; the builder's own checks are in
// chrome-text-media.spec.ts). Every template part's gallery runs the shared gallery checks; every component fixture
// page runs the same areas (reads at this size, axe in both themes, zoom, fonts blocked, motion, forced colours,
// print, no layout shift, JS only for the islands). Then the interactions Karthik decided: popovers open on click,
// tap, Enter and Space only; the gallery overlay and the video by keyboard and pointer; image failure in each engine.
import { deflateSync } from "node:zlib";
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
  test,
  watchLayoutShift,
  zoomTo200,
} from "../fixtures";
import {
  expectInsideViewport,
  expectNoClippedText,
  expectPopoverLook,
  expectPopoverPlacement,
  fixtureCase,
  galleryChecks,
  galleryPath,
  panelFor,
  tokenRgb,
} from "./primitives";

/** Case counts of each template part's fixtures/cases.tsx. */
const PARTS: Record<string, number> = {
  GlossaryTerm: 5,
  SkipLink: 1,
  Masthead: 4,
  DateLine: 3,
  RegionFilter: 4,
  Footer: 2,
  FlyTitle: 5,
  Headline: 8,
  Standfirst: 4,
  PageMeta: 5,
  Disclaimer: 2,
  Badge: 8,
  SourcesPanel: 5,
};
for (const [name, count] of Object.entries(PARTS)) galleryChecks(name, count);

const LIBRARY = ["Prose", "Quote", "Note", "MethodSummary", "ImageFigure", "ImageGallery", "VideoEmbed"];
const ISLANDS = new Set(["ImageGallery", "VideoEmbed"]);
const fixture = (name: string, f: string) => `/fixtures/components/${name}/${f}/`;

/** The box in page coordinates: focusing the player may scroll a short screen, which is not a layout shift. */
const pageBox = (el: Locator) =>
  el.evaluate((node) => {
    const r = node.getBoundingClientRect();
    return { x: r.left + window.scrollX, y: r.top + window.scrollY, width: r.width, height: r.height };
  });

/** Resolves once React has hydrated the island root (attached a fibre to its first element). */
async function hydrated(page: Page, island: string) {
  await expect
    .poll(() =>
      page
        .locator(`[data-island="${island}"] > *`)
        .first()
        .evaluate((el) => Object.keys(el).some((k) => k.startsWith("__reactFiber$"))),
    )
    .toBe(true);
}

for (const name of LIBRARY) {
  for (const f of ["default", "edge-cases"]) {
    const path = fixture(name, f);
    const id = `${name}/${f}`;

    test(`${id}: nothing outside the screen or clipped`, async ({ page }) => {
      await gotoPage(page, path);
      await expectNoSidewaysScroll(page);
      await expectInsideViewport(page);
      await expectNoClippedText(page);
      await expectNoSmallText(page);
      await expectTargetSize(page);
      await expectDesignFonts(page);
    });

    test(`${id}: reflows at 200% text and 200% zoom`, { tag: "@zoom" }, async ({ page }) => {
      await gotoPage(page, path);
      await setTextSize(page, 200);
      await expectNoSidewaysScroll(page);
      await expectInsideViewport(page);
      await expectNoClippedText(page);
      await setTextSize(page, 100);
      test.skip(!(await zoomTo200(page)), "below 640px wide, 200% zoom is the 320 project (400%)");
      await expectNoSidewaysScroll(page);
      await expectInsideViewport(page);
      await expectNoClippedText(page);
    });

    test(`${id}: JavaScript only for islands, and no layout shift once fonts are in`, async ({ page }) => {
      const scripts: string[] = [];
      page.on("request", (r) => {
        if (r.resourceType() === "script") scripts.push(r.url());
      });
      await gotoPage(page, path);
      if (ISLANDS.has(name)) await hydrated(page, name);
      const shift = await watchLayoutShift(page);
      if (!ISLANDS.has(name)) expect(scripts).toEqual([]);
      const total = await cumulativeLayoutShift(page);
      if (total !== null) {
        test.info().annotations.push({ type: "CLS since navigation, font swap included", description: String(total) });
        expect(await shift(), "layout shift after fonts loaded and islands hydrated").toBeLessThan(0.001);
      }
    });

    test(`${id}: nothing animates under reduced motion`, async ({ page }) => {
      await gotoPage(page, path);
      expect(await runningAnimations(page)).toEqual([]);
      expect(await longestMotionMs(page)).toBe(0);
    });

    test(`${id}: nothing animates on load; transitions within 200 ms`, { tag: "@motion" }, async ({ page }) => {
      await gotoPage(page, path);
      expect(await runningAnimations(page)).toEqual([]);
      expect(await longestMotionMs(page)).toBeLessThanOrEqual(200);
    });

    test(`${id}: keeps its content and passes axe in forced colours`, { tag: "@forced-colors" }, async ({ page }) => {
      await gotoPage(page, path);
      expect(await page.evaluate(() => matchMedia("(forced-colors: active)").matches)).toBe(true);
      await expectAccessible(page);
      await expectNoClippedText(page);
    });

    test(`${id}: prints its text, figures and terms; no popovers or controls`, { tag: "@print" }, async ({ page }) => {
      await gotoPage(page, path);
      const triggers = await page.locator("main button.popover-trigger").allInnerTexts();
      await page.emulateMedia({ media: "print" });
      for (const t of await page.locator("main button.popover-trigger").all()) await expect(t).toBeVisible();
      expect(await page.locator("main button.popover-trigger").allInnerTexts()).toEqual(triggers);
      await expect(page.locator("main [popover]:visible")).toHaveCount(0);
      await expect(page.locator(".video-embed__play:visible, .image-gallery__close:visible")).toHaveCount(0);
      await expectNoSidewaysScroll(page);
    });
  }
}

test.describe("GlossaryTerm (Karthik: click, tap, Enter, Space only)", () => {
  const PATH = fixture("Prose", "default");
  const term = (page: Page) => page.getByRole("button", { name: "gross margin" });

  test("hover and focus alone do not open the definition", { tag: "@interaction" }, async ({ page, isMobile }) => {
    await gotoPage(page, PATH);
    const panel = await panelFor(page, term(page));
    await term(page).focus();
    await page.waitForTimeout(250);
    await expect(panel).toBeHidden();
    test.skip(isMobile, "no hover on touch screens");
    await term(page).hover();
    await page.waitForTimeout(250);
    await expect(panel).toBeHidden();
  });

  test("Space opens it; Tab reaches the glossary link; Escape closes and focus returns", {
    tag: "@interaction",
  }, async ({ page, isMobile }) => {
    test.skip(isMobile, "no keyboard on touch screens");
    await gotoPage(page, PATH);
    const panel = await panelFor(page, term(page));
    await term(page).focus();
    await page.keyboard.press("Space");
    await expect(panel).toBeVisible();
    await expect(panel.getByRole("link", { name: "Read the glossary entry" })).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(panel).toBeHidden();
    await expect(term(page)).toBeFocused();
  });

  test("a click or tap opens it on screen next to the term, in the popover look of this theme", {
    tag: "@interaction",
  }, async ({ page, theme, hasTouch }) => {
    await gotoPage(page, PATH);
    const panel = await panelFor(page, term(page));
    if (hasTouch) await term(page).tap();
    else await term(page).click();
    await expect(panel).toBeVisible();
    await expectPopoverPlacement(page, term(page), panel);
    await expectPopoverLook(panel, theme);
    await expectAccessible(page);
    // A click or tap outside closes it.
    if (hasTouch) await page.locator("h1").tap();
    else await page.locator("h1").click();
    await expect(panel).toBeHidden();
  });

  test("the term has a dotted 1px tertiary-ink underline", async ({ page, theme }) => {
    await gotoPage(page, PATH);
    const s = await term(page).evaluate((el) => {
      const c = getComputedStyle(el);
      return [c.textDecorationLine, c.textDecorationStyle, c.textDecorationColor];
    });
    expect(s).toEqual(["underline", "dotted", tokenRgb("--color-ink-3", theme)]);
  });

  test("in the gallery, a term the glossary does not define is plain text", async ({ page }) => {
    await gotoPage(page, galleryPath("GlossaryTerm"));
    const c = fixtureCase(page, "a term the glossary does not define reads as plain text");
    await expect(c.locator("button")).toHaveCount(0);
  });
});

test.describe("ImageGallery overlay", () => {
  const PATH = fixture("ImageGallery", "default");

  test("a click or tap on the backdrop closes it and focus returns to the image", {
    tag: "@interaction",
  }, async ({ page, hasTouch }) => {
    await gotoPage(page, PATH);
    await hydrated(page, "ImageGallery");
    const first = page.locator(".image-gallery__open").first();
    if (hasTouch) await first.tap();
    else await first.click();
    const dialog = page.locator("dialog.image-gallery__overlay");
    await expect(dialog).toBeVisible();
    // The overlay figure and close button stay on screen at every size.
    const box = await dialog.locator(".image-gallery__full").boundingBox();
    const vp = page.viewportSize();
    if (!box || !vp) throw new Error("no geometry");
    expect(box.x).toBeGreaterThanOrEqual(-0.5);
    expect(box.x + box.width).toBeLessThanOrEqual(vp.width + 0.5);
    await expect(dialog.getByRole("button", { name: "Close" })).toBeInViewport();
    await expectNoSmallText(page, 12, dialog);
    // The backdrop: the dialog element itself, outside the figure.
    const d = await dialog.boundingBox();
    if (!d) throw new Error("no dialog box");
    await page.mouse.click(d.x + 2, d.y + 2);
    await expect(page.locator("dialog[open]")).toHaveCount(0);
  });

  test("Tab stays inside the open overlay (the page behind is inert)", { tag: "@interaction" }, async ({
    page,
    isMobile,
  }) => {
    test.skip(isMobile, "no keyboard on touch screens");
    await gotoPage(page, PATH);
    await hydrated(page, "ImageGallery");
    await page.locator(".image-gallery__open").first().click();
    for (let i = 0; i < 3; i++) {
      await page.keyboard.press("Tab");
      const inside = await page.evaluate(
        () => !!document.activeElement?.closest("dialog[open]") || document.activeElement === document.body,
      );
      expect(inside, "focus stays in the overlay or on the document").toBe(true);
    }
  });

  test("with JavaScript off each image is a link to the full-size file", async ({ browser }) => {
    const context = await browser.newContext({ javaScriptEnabled: false });
    const page = await context.newPage();
    await page.goto(PATH);
    const hrefs = await page.locator(".image-gallery__open").evaluateAll((as) => as.map((a) => a.getAttribute("href")));
    expect(hrefs.length).toBeGreaterThan(0);
    for (const h of hrefs) expect(h).toMatch(/^\/fixtures\/images\//);
    await context.close();
  });

  test("2 columns on phones, 3 from tablet width, 8px gaps", async ({ page }) => {
    await gotoPage(page, PATH);
    const s = await page
      .locator(".image-gallery__grid")
      .first()
      .evaluate((el) => {
        const c = getComputedStyle(el);
        return { cols: c.gridTemplateColumns.split(" ").length, gap: c.columnGap };
      });
    const width = page.viewportSize()?.width ?? 0;
    expect(s.cols).toBe(width < 600 ? 2 : 3);
    expect(s.gap).toBe("8px");
  });
});

test.describe("VideoEmbed", () => {
  test("Space and tap load the player in the same box", { tag: "@interaction" }, async ({ page, hasTouch }) => {
    await page.route("https://www.youtube-nocookie.com/**", (route) =>
      route.fulfill({ status: 200, contentType: "text/html", body: "<!doctype html><title>player</title>" }),
    );
    await gotoPage(page, fixture("VideoEmbed", "default"));
    await hydrated(page, "VideoEmbed");
    const frame = page.locator(".video-embed__frame");
    const before = await pageBox(frame);
    const play = page.getByRole("button", { name: /^Load video from YouTube: / });
    if (hasTouch) await play.tap();
    else {
      await play.focus();
      await page.keyboard.press("Space");
    }
    await expect(page.locator("iframe.video-embed__player")).toBeVisible();
    expect(await pageBox(frame)).toEqual(before);
  });

  test("the box is 16:9; the play button is a 56px circle that overlaps no text", async ({ page }) => {
    for (const f of ["default", "edge-cases"]) {
      await gotoPage(page, fixture("VideoEmbed", f));
      for (const fr of await page.locator(".video-embed__frame").all()) {
        const box = await fr.boundingBox();
        if (!box) throw new Error("no frame");
        expect(box.width / box.height).toBeCloseTo(16 / 9, 1);
        const play = await fr.locator(".video-embed__play").boundingBox();
        if (!play) throw new Error("no play button");
        expect(Math.round(play.width)).toBe(56);
        // Since VID-1 the text is the caption under the box (a <p> inside the box would be a regression).
        await expect(fr.locator("p")).toHaveCount(0);
        const caption = fr.locator("xpath=ancestor::figure[1]/figcaption/*");
        expect(await caption.count()).toBe(3);
        for (const t of await caption.all()) {
          const tb = await t.boundingBox();
          if (!tb) continue;
          const overlap =
            play.x < tb.x + tb.width &&
            play.x + play.width > tb.x &&
            play.y < tb.y + tb.height &&
            play.y + play.height > tb.y;
          expect(overlap, `${f}: play button overlaps "${(await t.innerText()).slice(0, 30)}"`).toBe(false);
        }
      }
    }
  });

  test.describe(() => {
    test.use({ allowedConsoleErrors: [[/youtube/, /Failed to load resource/, /ERR_/], { scope: "test" }] as never });
    test("offline: the player request fails and the page still reads with no shift", async ({ page }) => {
      await page.route("https://www.youtube-nocookie.com/**", (route) => route.abort());
      await gotoPage(page, fixture("VideoEmbed", "default"));
      await hydrated(page, "VideoEmbed");
      const frame = page.locator(".video-embed__frame");
      const before = await pageBox(frame);
      await page.getByRole("button", { name: /^Load video from YouTube: / }).click();
      await expect(page.locator("iframe.video-embed__player")).toBeAttached();
      expect(await pageBox(frame)).toEqual(before);
      await expectNoSidewaysScroll(page);
    });
  });
});

test.describe("ImageFigure failure in every engine", () => {
  const BLOCKED = /\/fixtures\/images\//;
  test.use({ allowedConsoleErrors: [[/fixtures\/images/, /Failed to load resource/], { scope: "test" }] as never });

  test("every blocked image keeps its frame size and its credit stays readable under it", async ({ page }) => {
    await page.route(BLOCKED, (route) => route.abort());
    await gotoPage(page, fixture("ImageFigure", "edge-cases"));
    for (const fig of await page.locator("figure.image-figure").all()) {
      await fig.scrollIntoViewIfNeeded();
      const frame = fig.locator(".image-frame");
      const ratio = await frame.evaluate((el) => {
        const s = getComputedStyle(el);
        return Number(s.getPropertyValue("--image-w")) / Number(s.getPropertyValue("--image-h"));
      });
      const box = await frame.boundingBox();
      if (!box) throw new Error("no frame");
      expect(box.width / box.height).toBeCloseTo(ratio, 1);
      await expect(fig.locator(".image-figure__credit")).toBeVisible();
    }
    await expectNoSidewaysScroll(page);
  });

  test("a blocked image's frame shows its alt text (screenshot attached for the engine)", async ({
    page,
    browserName,
  }, info) => {
    await page.route(BLOCKED, (route) => route.abort());
    await gotoPage(page, fixture("ImageFigure", "default"));
    const frame = page.locator(".image-frame").first();
    await frame.scrollIntoViewIfNeeded();
    const shot = await frame.screenshot({ path: info.outputPath(`blocked-image-${browserName}.png`) });
    await info.attach(`blocked-image-${browserName}`, { body: shot, contentType: "image/png" });
    // Chromium and Firefox draw the ::after fallback over the broken image. WebKit computes ::after but does not
    // draw it on a broken image; there the frame's ::before layer behind the image shows through (IMG-1). The
    // attached screenshot is the evidence that it is drawn.
    const after = await page
      .locator(".image-frame__img")
      .first()
      .evaluate((el) => getComputedStyle(el, "::after").content);
    expect(after).toContain("elliptical orbit");
    expect(after).toContain("NASA/JPL-Caltech. Public domain.");
    const behind = await frame.evaluate((el) => {
      const s = getComputedStyle(el, "::before");
      return { content: s.content, position: s.position, color: s.color, background: s.backgroundColor };
    });
    expect(behind.content).toContain("elliptical orbit");
    expect(behind.content).toContain("NASA/JPL-Caltech. Public domain.");
    expect(behind.position).toBe("absolute");
    expect(behind.color).not.toBe(behind.background);
  });
});

test.describe("chrome parts", () => {
  test("Masthead: one row from 600px; the links wrap under the wordmark on phones", async ({ page }) => {
    await gotoPage(page, galleryPath("Masthead"));
    const header = page.locator("header.masthead").first();
    const word = await header.locator(".masthead__wordmark").boundingBox();
    const nav = await header.locator(".masthead__nav").boundingBox();
    if (!word || !nav) throw new Error("no masthead");
    const width = page.viewportSize()?.width ?? 0;
    if (width < 600) expect(nav.y).toBeGreaterThan(word.y + word.height - 1);
    else expect(nav.y).toBeLessThan(word.y + word.height);
  });

  test("Masthead and RegionFilter: crimson only on the current link", async ({ page, theme }) => {
    for (const [path, sel] of [
      [galleryPath("Masthead"), ".masthead__link"],
      [galleryPath("RegionFilter"), ".region-filter__link"],
    ] as const) {
      await gotoPage(page, path);
      const brand = tokenRgb("--color-brand", theme);
      const links = await page.locator(sel).evaluateAll((els) =>
        els.map((el) => ({
          current: el.getAttribute("aria-current") === "page",
          border: getComputedStyle(el).borderBottomColor,
          text: getComputedStyle(el).color,
        })),
      );
      for (const l of links) {
        expect(l.text, `${sel} text`).not.toBe(brand);
        if (!l.current) expect(l.border, `${sel} underline`).not.toBe(brand);
      }
      expect(
        links.some((l) => l.current && l.border === brand),
        `${sel}: a current link`,
      ).toBe(true);
    }
  });

  test("SkipLink: hidden until focused, then on screen in paper on ink", async ({
    page,
    theme,
    isMobile,
    browserName,
  }) => {
    test.skip(isMobile, "no Tab key on touch screens");
    await gotoPage(page, galleryPath("SkipLink"));
    const link = page.getByRole("link", { name: "Skip to content" });
    const hidden = await link.boundingBox();
    expect((hidden?.y ?? 0) + (hidden?.height ?? 0)).toBeLessThanOrEqual(1);
    // First in the tab order in Chromium and Firefox. Playwright WebKit on Windows does not move to links with Tab
    // (nor Alt+Tab), as in Safari by default, so there the link is focused directly to check how it shows.
    if (browserName === "webkit") await link.focus();
    else await page.keyboard.press("Tab");
    await expect(link).toBeFocused();
    await expect(link).toBeInViewport();
    const s = await link.evaluate((el) => [getComputedStyle(el).color, getComputedStyle(el).backgroundColor]);
    expect(s).toEqual([tokenRgb("--color-paper", theme), tokenRgb("--color-ink", theme)]);
  });

  test("Badge: the text says it; Primary in focus blue, Context in ink-3, the rest ink-2", async ({ page, theme }) => {
    await gotoPage(page, galleryPath("Badge"));
    const colours = await page
      .locator(".badge")
      .evaluateAll((els) =>
        els.map((el) => [
          el.className,
          el.textContent,
          getComputedStyle(el).color,
          getComputedStyle(el).borderTopColor,
        ]),
      );
    expect(colours.length).toBeGreaterThan(0);
    for (const [cls, text, color, border] of colours) {
      expect(text?.trim()).not.toBe("");
      expect(border).toBe(color);
      const want = cls?.includes("primary")
        ? "--color-focus"
        : cls?.includes("context")
          ? "--color-ink-3"
          : "--color-ink-2";
      expect(color, String(cls)).toBe(tokenRgb(want, theme));
    }
  });

  test("Disclaimer and PageMeta use their ink colours, never a box", async ({ page, theme }) => {
    await gotoPage(page, galleryPath("Disclaimer"));
    for (const d of await page.locator(".disclaimer").all()) {
      const s = await d.evaluate((el) => {
        const c = getComputedStyle(el);
        return [c.color, c.borderTopStyle, c.backgroundColor];
      });
      expect(s[0]).toBe(tokenRgb("--color-ink-3", theme));
      expect(s[1]).toBe("none");
      expect(s[2]).toBe("rgba(0, 0, 0, 0)");
    }
    await gotoPage(page, galleryPath("PageMeta"));
    const updated = page.locator(".page-meta__updated a").first();
    expect(await updated.evaluate((el) => getComputedStyle(el).color)).toBe(tokenRgb("--color-ink", theme));
  });

  test("SourcesPanel: a 2px ink rule on top; Updated on links to the change note", async ({ page, theme }) => {
    await gotoPage(page, galleryPath("SourcesPanel"));
    const panel = page.locator(".sources-panel").first();
    const rule = await panel.evaluate((el) => [
      getComputedStyle(el).borderTopWidth,
      getComputedStyle(el).borderTopColor,
    ]);
    expect(rule).toEqual(["2px", tokenRgb("--color-ink", theme)]);
    const ids = await page.locator(".sources-panel__change").evaluateAll((els) => els.map((e) => e.id));
    expect(ids.length).toBeGreaterThan(0);
    for (const id of ids) expect(id).toMatch(/change-note$/);
  });
});

// Re-test of issue #7's fix round (VID-1, IMG-1, PROSE-1), by a tester that did not build the fixes. Test names
// carry the defect id, so `-g "VID-1|IMG-1|PROSE-1"` runs just these.

/** Every caption line of every embedded video: below the 16:9 box, on screen, not clipped, clear of the button. */
async function expectVideoCaptionsClear(page: Page, label: string) {
  const vp = page.viewportSize();
  if (!vp) throw new Error("no viewport");
  const figures = await page.locator("figure.video-embed").all();
  expect(figures.length, `${label}: embedded videos on the page`).toBeGreaterThan(0);
  for (const fig of figures) {
    await fig.scrollIntoViewIfNeeded();
    const frame = await pageBox(fig.locator(".video-embed__frame"));
    const play = await pageBox(fig.locator(".video-embed__play, iframe.video-embed__player").first());
    const lines = fig.locator("figcaption.video-embed__caption > *");
    await expect(lines).toHaveCount(3);
    for (const line of await lines.all()) {
      const text = (await line.innerText()).slice(0, 40);
      await expect(line, `${label}: "${text}" visible`).toBeVisible();
      const b = await pageBox(line);
      expect(b.y, `${label}: "${text}" sits under the box`).toBeGreaterThanOrEqual(frame.y + frame.height - 0.5);
      expect(b.y >= play.y + play.height || b.y + b.height <= play.y, `${label}: "${text}" clear of the button`).toBe(
        true,
      );
      expect(b.x, `${label}: "${text}" starts on screen`).toBeGreaterThanOrEqual(-0.5);
      expect(b.x + b.width, `${label}: "${text}" ends on screen`).toBeLessThanOrEqual(vp.width + 0.5);
      // Cut means a box that hides overflow (the line, the caption or the figure) is smaller than its content.
      // Glyphs reaching a pixel or two past a visible-overflow line box (Literata's descenders) are not cut.
      const clip = await line.evaluate((el) => {
        const cut: string[] = [];
        for (let n: HTMLElement | null = el as HTMLElement; n && n.tagName !== "MAIN"; n = n.parentElement) {
          const s = getComputedStyle(n);
          if (!/hidden|clip/.test(s.overflowX + s.overflowY)) continue;
          if (n.scrollWidth > n.clientWidth + 1 || n.scrollHeight > n.clientHeight + 1) cut.push(n.className);
        }
        return { cut, clamp: getComputedStyle(el).webkitLineClamp };
      });
      expect(clip.cut, `${label}: "${text}" cut by an overflow box`).toEqual([]);
      expect(clip.clamp, `${label}: "${text}" line-clamped`).toMatch(/^(none|)$/);
    }
  }
}

test.describe("Fix round VID-1: the video's text is a caption under the box", () => {
  for (const f of ["default", "edge-cases"]) {
    test(`VID-1 ${f}: caption under the box, whole, on screen, at 100% and 200% text`, async ({ page }) => {
      await gotoPage(page, fixture("VideoEmbed", f));
      await expectVideoCaptionsClear(page, "100% text");
      await setTextSize(page, 200);
      await expectVideoCaptionsClear(page, "200% text");
      await expectNoSidewaysScroll(page);
    });

    test(`VID-1 ${f}: caption readable in forced colours, button still visible`, {
      tag: "@forced-colors",
    }, async ({ page }) => {
      await gotoPage(page, fixture("VideoEmbed", f));
      await expectVideoCaptionsClear(page, "forced colours");
      for (const line of await page.locator("figcaption.video-embed__caption > *").all()) {
        const c = await line.evaluate((el) => {
          let bg = "rgba(0, 0, 0, 0)";
          for (let n: Element | null = el; n && /rgba\(0, 0, 0, 0\)|transparent/.test(bg); n = n.parentElement)
            bg = getComputedStyle(n).backgroundColor;
          return { color: getComputedStyle(el).color, bg };
        });
        expect(c.color).not.toBe(c.bg);
      }
      for (const play of await page.locator(".video-embed__play").all()) await expect(play).toBeVisible();
      await setTextSize(page, 200);
      await expectVideoCaptionsClear(page, "forced colours, 200% text");
    });

    test(`VID-1 ${f}: print keeps the title, publisher and date only`, { tag: "@print" }, async ({ page }) => {
      await gotoPage(page, fixture("VideoEmbed", f));
      await page.emulateMedia({ media: "print" });
      for (const fig of await page.locator("figure.video-embed").all()) {
        await expect(fig.locator(".video-embed__frame")).toBeHidden();
        await expect(fig.locator(".video-embed__notice")).toBeHidden();
        await expect(fig.locator(".video-embed__title")).toBeVisible();
        await expect(fig.locator(".video-embed__meta")).toBeVisible();
      }
      for (const a of await page.locator("p.video-embed--link a").all()) await expect(a).toBeVisible();
      await expectNoSidewaysScroll(page);
    });
  }

  test("VID-1: after the click the caption stays under the player", { tag: "@interaction" }, async ({ page }) => {
    await page.route("https://www.youtube-nocookie.com/**", (route) =>
      route.fulfill({ status: 200, contentType: "text/html", body: "<!doctype html><title>player</title>" }),
    );
    await gotoPage(page, fixture("VideoEmbed", "default"));
    await hydrated(page, "VideoEmbed");
    const caption = page.locator("figcaption.video-embed__caption");
    const before = await pageBox(caption);
    await page.getByRole("button", { name: /^Load video from YouTube: / }).click();
    await expect(page.locator("iframe.video-embed__player")).toBeVisible();
    expect(await pageBox(caption)).toEqual(before);
    await expectVideoCaptionsClear(page, "player loaded");
  });
});

/** True when the frame's fallback text is visible: the frame looks different with both fallback layers removed. */
async function fallbackVisible(page: Page, frame: Locator): Promise<boolean> {
  await frame.scrollIntoViewIfNeeded();
  const withLayer = await frame.screenshot({ animations: "disabled" });
  const style = await page.addStyleTag({
    content: ".image-frame::before, .image-frame__img::after { content: none !important; }",
  });
  const without = await frame.screenshot({ animations: "disabled" });
  await style.evaluate((el) => el.remove());
  const again = await frame.screenshot({ animations: "disabled" });
  expect(again.equals(withLayer), "the frame renders the same twice (a stable comparison)").toBe(true);
  return !withLayer.equals(without);
}

/** A 300 x 150 PNG, transparent except an opaque black square in the middle (a logo or a chart on no background). */
function transparentPng(): Buffer {
  const w = 300;
  const h = 150;
  const table = Array.from({ length: 256 }, (_, n) => {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    return c >>> 0;
  });
  const crc = (b: Buffer) => {
    let c = 0xffffffff;
    for (const x of b) c = table[(c ^ x) & 0xff] ^ (c >>> 8);
    return (c ^ 0xffffffff) >>> 0;
  };
  const chunk = (type: string, data: Buffer) => {
    const len = Buffer.alloc(4);
    len.writeUInt32BE(data.length);
    const body = Buffer.concat([Buffer.from(type), data]);
    const sum = Buffer.alloc(4);
    sum.writeUInt32BE(crc(body));
    return Buffer.concat([len, body, sum]);
  };
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(w, 0);
  ihdr.writeUInt32BE(h, 4);
  ihdr[8] = 8; // bit depth
  ihdr[9] = 6; // RGBA
  const raw = Buffer.alloc((w * 4 + 1) * h);
  for (let y = 0; y < h; y++)
    for (let x = 0; x < w; x++)
      if (x > w * 0.4 && x < w * 0.6 && y > h * 0.3 && y < h * 0.7) raw[y * (w * 4 + 1) + 1 + x * 4 + 3] = 255;
  const sig = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);
  return Buffer.concat([sig, chunk("IHDR", ihdr), chunk("IDAT", deflateSync(raw)), chunk("IEND", Buffer.alloc(0))]);
}

test.describe("Fix round IMG-1: the image fallback layer", () => {
  const IMAGES = /\/fixtures\/images\//;

  test.describe(() => {
    test.use({ allowedConsoleErrors: [[/fixtures\/images/, /Failed to load resource/], { scope: "test" }] as never });

    test("IMG-1: a blocked image shows its alt text and credit in its frame, in this engine", async ({
      page,
      browserName,
    }, info) => {
      await page.route(IMAGES, (route) => route.abort());
      await gotoPage(page, fixture("ImageFigure", "default"));
      const frame = page.locator(".image-frame").first();
      expect(await fallbackVisible(page, frame), `fallback drawn in ${browserName}`).toBe(true);
      const shot = await frame.screenshot();
      await info.attach(`blocked-image-${browserName}`, { body: shot, contentType: "image/png" });
      // The frame keeps the image's ratio; the text is Fira 13.5 secondary ink on the wash.
      const box = await frame.boundingBox();
      if (!box) throw new Error("no frame");
      expect(box.width / box.height).toBeCloseTo(1600 / 900, 1);
    });

    test("IMG-1: a blocked gallery image (natural and square crops) shows its fallback, in this engine", async ({
      page,
    }) => {
      await page.route(IMAGES, (route) => route.abort());
      await gotoPage(page, fixture("ImageGallery", "default"));
      expect(await fallbackVisible(page, page.locator(".image-gallery__grid .image-frame").first())).toBe(true);
      await gotoPage(page, fixture("ImageGallery", "edge-cases"));
      expect(await fallbackVisible(page, page.locator(".image-gallery__grid .image-frame--square").first())).toBe(true);
    });
  });

  test("IMG-1: a loaded opaque image hides the fallback completely", async ({ page }) => {
    await gotoPage(page, fixture("ImageFigure", "edge-cases"));
    for (const frame of await page.locator(".image-frame").all()) {
      await frame.locator("img").evaluate((img: HTMLImageElement) => img.decode());
      expect(await fallbackVisible(page, frame)).toBe(false);
    }
  });

  test("IMG-1: hovering a gallery image does not show the fallback text through it", async ({ page, hasTouch }) => {
    test.skip(hasTouch, "no hover on touch screens");
    await gotoPage(page, fixture("ImageGallery", "default"));
    const link = page.locator(".image-gallery__open").first();
    await link.locator("img").evaluate((img: HTMLImageElement) => img.decode());
    await link.hover();
    expect(await fallbackVisible(page, link.locator(".image-frame"))).toBe(false);
  });

  test("IMG-1: while an image is still loading its frame shows the alt text (documented), then the image", async ({
    page,
  }) => {
    let release: () => void = () => {};
    const gate = new Promise<void>((r) => {
      release = r;
    });
    await page.route(IMAGES, async (route) => {
      await gate;
      await route.continue();
    });
    await page.goto(fixture("ImageFigure", "default"), { waitUntil: "domcontentloaded" });
    const frame = page.locator(".image-frame").first();
    const before = await frame.boundingBox();
    expect(await frame.locator("img").evaluate((img: HTMLImageElement) => img.complete)).toBe(false);
    // Fonts may wait for the page's load event, which the held image delays: compare by computed layer instead.
    const layer = await frame.evaluate((el) => getComputedStyle(el, "::before").content);
    expect(layer).toContain("elliptical orbit");
    release();
    await frame.locator("img").evaluate((img: HTMLImageElement) => img.decode());
    await page.evaluate(() => document.fonts.ready.then(() => undefined));
    expect(await frame.boundingBox()).toEqual(before);
    expect(await fallbackVisible(page, frame)).toBe(false);
  });

  test("IMG-1 (b): a loaded transparent PNG must not show the fallback text through it", async ({ page }) => {
    test.fail(true, "IMG-1b open: the ::before layer shows through transparent pixels in every engine");
    await page.route(IMAGES, (route) =>
      route.fulfill({ status: 200, contentType: "image/png", body: transparentPng() }),
    );
    await gotoPage(page, fixture("ImageFigure", "default"));
    const frame = page.locator(".image-frame").first();
    await frame.locator("img").evaluate((img: HTMLImageElement) => img.decode());
    expect(await fallbackVisible(page, frame)).toBe(false);
  });
});

test.describe("Fix round PROSE-1: heading ids in the browser", () => {
  test("PROSE-1: every id on the Prose fixture pages is unique and each heading link lands on its heading", async ({
    page,
  }) => {
    for (const f of ["default", "edge-cases"]) {
      await gotoPage(page, fixture("Prose", f));
      const ids = await page.evaluate(() => [...document.querySelectorAll("[id]")].map((e) => e.id));
      expect(
        ids.filter((v, i) => ids.indexOf(v) !== i),
        `${f}: duplicate ids`,
      ).toEqual([]);
      for (const h of await page.locator("main h2[id^='section-']").all()) {
        const id = await h.getAttribute("id");
        expect(await page.evaluate((x) => document.getElementById(x as string)?.tagName, id)).toBe("H2");
      }
    }
  });
});
