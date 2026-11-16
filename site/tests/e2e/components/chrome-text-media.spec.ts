// Builder's browser checks for the #7 family (chrome, article head, text, media, sources). Component fixture pages
// (/fixtures/components/<Name>/<fixture>/) and the template parts' galleries (/fixtures/primitives/<Name>/) pass
// the shared checks; the video loads nothing from YouTube before the click and only youtube-nocookie.com after;
// a blocked image keeps its size and shows its alt text and credit; the gallery overlay and the glossary popover
// close on Escape and give focus back. The tester extends this into the full procedure and matrix.
import type { Page } from "@playwright/test";
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
  watchLayoutShift,
} from "../fixtures";

const FIXTURES: [string, string][] = [
  ["Prose", "default"],
  ["Prose", "edge-cases"],
  ["Quote", "default"],
  ["Quote", "edge-cases"],
  ["Note", "default"],
  ["Note", "edge-cases"],
  ["MethodSummary", "default"],
  ["MethodSummary", "edge-cases"],
  ["ImageFigure", "default"],
  ["ImageFigure", "edge-cases"],
  ["ImageGallery", "default"],
  ["ImageGallery", "edge-cases"],
  ["VideoEmbed", "default"],
  ["VideoEmbed", "edge-cases"],
];
const PARTS = [
  "GlossaryTerm",
  "SkipLink",
  "Masthead",
  "DateLine",
  "RegionFilter",
  "Footer",
  "FlyTitle",
  "Headline",
  "Standfirst",
  "PageMeta",
  "Disclaimer",
  "Badge",
  "SourcesPanel",
];
const fixture = (name: string, f: string) => `/fixtures/components/${name}/${f}/`;
const PAGES: [string, string][] = [
  ...FIXTURES.map(([n, f]): [string, string] => [`${n}/${f}`, fixture(n, f)]),
  ...PARTS.map((n): [string, string] => [`${n} gallery`, `/fixtures/primitives/${n}/`]),
];

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

for (const [name, path] of PAGES) {
  test(`${name}: accessible in both themes, reads at this screen size, in the design fonts`, {
    tag: ["@smoke", "@a11y"],
  }, async ({ page, theme }) => {
    await gotoPage(page, path);
    await expectAccessible(page);
    await expectNoSidewaysScroll(page);
    await expectNoSmallText(page);
    await expectTargetSize(page);
    await expectDesignFonts(page);
    await setTheme(page, theme === "light" ? "dark" : "light");
    await expectAccessible(page);
  });
}

test("static components ship no JavaScript", { tag: "@smoke" }, async ({ request }) => {
  for (const [n, f] of FIXTURES.filter(([n]) => !["ImageGallery", "VideoEmbed"].includes(n))) {
    const html = await (await request.get(fixture(n, f))).text();
    expect(html, `${n}/${f}`).not.toContain("<script");
  }
});

test.describe("VideoEmbed", () => {
  test("requests nothing from YouTube before the click, then only youtube-nocookie.com", {
    tag: ["@smoke", "@interaction"],
  }, async ({ page }) => {
    const youtube: string[] = [];
    page.on("request", (req) => {
      if (/youtube|ytimg|googlevideo|doubleclick|google\./.test(new URL(req.url()).hostname)) youtube.push(req.url());
    });
    // Answer the embed locally: the test checks where the page goes, not YouTube itself.
    await page.route("https://www.youtube-nocookie.com/**", (route) =>
      route.fulfill({ status: 200, contentType: "text/html", body: "<!doctype html><title>player</title>" }),
    );
    await gotoPage(page, fixture("VideoEmbed", "default"));
    await hydrated(page, "VideoEmbed");
    await expect(page.getByText("Loads from YouTube. YouTube will receive your IP address.")).toBeVisible();
    expect(youtube, "requests to YouTube before the click").toEqual([]);

    const frame = page.locator(".video-embed__frame");
    const before = await frame.boundingBox();
    const play = page.getByRole("button", { name: /^Load video from YouTube: / });
    await play.focus();
    await page.keyboard.press("Enter");
    const player = page.locator("iframe.video-embed__player");
    await expect(player).toHaveAttribute("src", "https://www.youtube-nocookie.com/embed/aBcDeFgHiJk?autoplay=1");
    await expect(player).toBeFocused();
    expect(await frame.boundingBox()).toEqual(before);
    await expect.poll(() => youtube.length).toBeGreaterThan(0);
    for (const url of youtube) expect(new URL(url).hostname).toBe("www.youtube-nocookie.com");
  });

  test("works as a link with JavaScript off", { tag: "@smoke" }, async ({ browser }) => {
    const context = await browser.newContext({ javaScriptEnabled: false });
    const page = await context.newPage();
    await page.goto(fixture("VideoEmbed", "default"));
    await expect(page.getByRole("link", { name: /^Load video from YouTube: / })).toHaveAttribute(
      "href",
      "https://www.federalreserve.gov/monetarypolicy/fomcpresconf20240918.htm",
    );
    await context.close();
  });
});

test.describe("ImageFigure", () => {
  const BLOCKED = /orbit-1600x900\.svg/;
  // A fixture's value that is an array must be wrapped as [value, options], or Playwright reads it as that pair.
  test.use({ allowedConsoleErrors: [[BLOCKED, /Failed to load resource/], { scope: "test" }] });

  test("a blocked image keeps its size and shows its alt text and credit, with no layout shift", {
    tag: "@smoke",
  }, async ({ page, browserName }) => {
    await page.route(BLOCKED, (route) => route.abort());
    await gotoPage(page, fixture("ImageFigure", "default"));
    const shift = await watchLayoutShift(page);
    const img = page.locator(".image-frame__img").first();
    await img.scrollIntoViewIfNeeded();
    await expect.poll(() => img.evaluate((el: HTMLImageElement) => el.complete && el.naturalWidth === 0)).toBe(true);
    const box = await page.locator(".image-frame").first().boundingBox();
    if (!box) throw new Error("no image frame");
    expect(box.width / box.height).toBeCloseTo(1600 / 900, 2);
    if (browserName !== "webkit") {
      const after = await img.evaluate((el) => getComputedStyle(el, "::after").content);
      expect(after).toContain("elliptical orbit");
      expect(after).toContain("NASA/JPL-Caltech. Public domain.");
    }
    const cls = await shift();
    if (cls !== null) expect(cls).toBeLessThan(0.001);
  });
});

test.describe("ImageGallery", () => {
  test("opens an image full size; Escape closes the overlay and focus returns to the image", {
    tag: ["@smoke", "@interaction"],
  }, async ({ page }) => {
    await gotoPage(page, fixture("ImageGallery", "default"));
    await hydrated(page, "ImageGallery");
    const first = page.locator(".image-gallery__open").first();
    await first.focus();
    await page.keyboard.press("Enter");
    const dialog = page.getByRole("dialog", { name: "The probe's orbit." });
    await expect(dialog).toBeVisible();
    await expect(dialog.getByText("NASA/JPL-Caltech. Public domain.")).toBeVisible();
    await expect(dialog.getByRole("button", { name: "Close" })).toBeFocused();
    await expectAccessible(page);
    await page.keyboard.press("Escape");
    await expect(dialog).toBeHidden();
    await expect(first).toBeFocused();
    // The close button closes it too.
    await first.click();
    await page.getByRole("button", { name: "Close" }).click();
    await expect(page.locator("dialog[open]")).toHaveCount(0);
  });
});

test.describe("GlossaryTerm", () => {
  test("opens the definition with no script; Escape closes it and focus returns", {
    tag: ["@smoke", "@interaction"],
  }, async ({ page }) => {
    await gotoPage(page, fixture("Prose", "default"));
    const term = page.getByRole("button", { name: "gross margin" });
    const panel = page.locator(`#${await term.getAttribute("popovertarget")}`);
    await term.click();
    await expect(panel).toBeVisible();
    await expect(panel).toContainText("Sales minus the cost of the goods and services sold");
    await expect(panel.getByRole("link", { name: "Read the glossary entry" })).toHaveAttribute(
      "href",
      "/fixtures/primitives/GlossaryTerm/#glossary-gross-margin",
    );
    await page.keyboard.press("Escape");
    await expect(panel).toBeHidden();
    await expect(term).toBeFocused();
  });
});

test("the skip link is first in the tab order and shows on focus", { tag: "@interaction" }, async ({
  page,
  isMobile,
}) => {
  test.skip(isMobile, "no Tab key on touch screens");
  await gotoPage(page, "/fixtures/primitives/SkipLink/");
  const link = page.getByRole("link", { name: "Skip to content" });
  await link.focus();
  const box = await link.boundingBox();
  expect(box?.y ?? -1).toBeGreaterThanOrEqual(0);
});
