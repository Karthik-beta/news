// Chart social images (#13; design/charts.md, "Social images"): the PNG scripts/social-image.ts writes is 1200×675,
// light mode, and has no text under 20px, for a column chart and a map. It opens its own context at the card's size,
// so it runs once, in the light 1440 Chromium project.
import { SOCIAL, smallestTextPx, socialImage } from "../../../scripts/social-image";
import { expect, test } from "../fixtures";

const CHARTS = [
  ["/fixtures/components/ColumnChart/default/", "services"],
  ["/fixtures/components/ChoroplethMap/default/", "jobless"],
] as const;

for (const [path, chart] of CHARTS) {
  test(
    `social image of ${chart}: 1200×675, light, no text under 20px`,
    { tag: "@smoke" },
    async ({ browser, browserName, theme, baseURL }, info) => {
      test.skip(browserName !== "chromium" || theme !== "light" || info.project.use.viewport?.width !== 1440, "once");
      const { png, page } = await socialImage(browser, { base: baseURL as string, path, chart });
      // PNG header: width and height are the big-endian words at bytes 16 and 20.
      expect([png.readUInt32BE(16), png.readUInt32BE(20)]).toEqual([SOCIAL.width, SOCIAL.height]);
      const smallest = await smallestTextPx(page);
      expect(smallest.px, smallest.text).toBeGreaterThanOrEqual(SOCIAL.minTextPx);
      await expect(page.locator(".social-card__url")).toHaveText(`news.karthik.nz${path}`);
      await expect(page.locator(".social-card__wordmark")).toHaveText("News");
      expect(await page.evaluate(() => getComputedStyle(document.body).colorScheme)).toContain("light");
      await page.context().close();
    },
  );
}
