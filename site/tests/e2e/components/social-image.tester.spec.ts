// Component tester's checks for chart social images (#13; design/charts.md, "Social images"), on top of the builder's
// social-image.spec.ts: a line chart, a heatmap, small multiples (two panels side by side, which used to miss the card
// by 7px because the height was shared per panel, not per row) and a map each make a 1200×675 light PNG with no text
// under 20px, the plot inside the card's 48px padding; a Sankey, a long waterfall and a 16-row bar chart, too tall
// for that, make a 1200×1200 square by the same rules (#71); a chart too tall for a card, or with nothing to show,
// fails with an error that names the chart and says why, never a shrunken or empty card. It opens its own contexts at the
// card's size, so it runs once, in the light 1440 Chromium project.
import { layoutSocialCard, SOCIAL, smallestTextPx, socialImage, TooTallError } from "../../../scripts/social-image";
import { expect, test } from "../fixtures";

const WIDE = SOCIAL.height;
const SQUARE = SOCIAL.squareHeight;
// Charts too tall for the 16:9 card get the 1200×1200 square (#71; design/charts.md, "Tall charts get a square").
const FITS = [
  ["/fixtures/components/LineChart/default/", "margins", WIDE],
  ["/fixtures/components/Heatmap/default/", "margins", WIDE],
  ["/fixtures/components/SmallMultiples/default/", "margins", WIDE],
  ["/fixtures/components/SmallMultiples/default/", "columns", WIDE],
  ["/fixtures/components/ChoroplethMap/default/", "g7", WIDE],
  ["/fixtures/components/ChoroplethMap/edge-cases/", "tiny", WIDE],
  ["/fixtures/components/Sankey/default/", "flows", SQUARE],
  ["/fixtures/components/Waterfall/default/", "profit", SQUARE],
  ["/fixtures/components/BarChart/edge-cases/", "long", SQUARE],
] as const;

const FAILS = [
  ["/fixtures/components/ChoroplethMap/edge-cases/", "allnull", /^chart "allnull" has no data to show$/],
  ["/fixtures/components/LineChart/default/", "nope", /^no chart "nope" on the page$/],
] as const;

test.describe("social images across chart types", () => {
  test.beforeEach(({ browserName, theme }, info) => {
    test.skip(browserName !== "chromium" || theme !== "light" || info.project.use.viewport?.width !== 1440, "once");
  });

  for (const [path, chart, height] of FITS) {
    test(`${path} ${chart}: 1200×${height}, every part inside the padding, no text under 20px`, async ({
      browser,
      baseURL,
    }) => {
      const { png, page } = await socialImage(browser, { base: baseURL as string, path, chart });
      expect([png.readUInt32BE(16), png.readUInt32BE(20)]).toEqual([SOCIAL.width, height]);
      const smallest = await smallestTextPx(page);
      expect(smallest.px, smallest.text).toBeGreaterThanOrEqual(SOCIAL.minTextPx);
      // 48 image px of padding is 28.8 CSS px: the figure and the footer sit inside it, nothing spills. Accepted
      // exception (SIGNOFF.md): an x label centred under the last column may hang into the padding, as it hangs into
      // the page's gutter on the site (SmallMultiples "columns": "fiscal 2024" by 5 image px), never past half of it.
      const out = await page.evaluate(() => {
        const card = document.querySelector(".social-card") as HTMLElement;
        const pad = Number.parseFloat(getComputedStyle(card).paddingLeft);
        return [...card.querySelectorAll<HTMLElement>("figure *, .social-card__foot")]
          .filter((el) => el.getClientRects().length > 0 && el.getBoundingClientRect().width > 1)
          .filter((el) => {
            const p = el.matches(".chart-plot__x-label") ? pad / 2 : pad;
            const r = el.getBoundingClientRect();
            return (
              r.left < p - 0.5 ||
              r.right > card.clientWidth - p + 0.5 ||
              r.top < p - 0.5 ||
              r.bottom > card.clientHeight - p + 0.5
            );
          })
          .map((el) => `${el.tagName.toLowerCase()}.${el.getAttribute("class")}`);
      });
      expect(out, "outside the card's padding").toEqual([]);
      await page.context().close();
    });
  }

  for (const [path, chart, message] of FAILS) {
    test(`${path} ${chart}: fails with a clear error, never a shrunken or empty card`, async ({ browser, baseURL }) => {
      await expect(socialImage(browser, { base: baseURL as string, path, chart })).rejects.toThrow(message);
    });
  }

  // Too tall is its own error, the one that moves a chart to the square; no fixture chart outgrows the square at 100%
  // text, so the square case doubles the text size, which makes the 16-row bar chart about twice as tall.
  const TOO_TALL = [
    ["/fixtures/components/Sankey/default/", "flows", WIDE, "100%"],
    ["/fixtures/components/BarChart/edge-cases/", "long", SQUARE, "200%"],
  ] as const;
  for (const [path, chart, height, text] of TOO_TALL) {
    test(`${path} ${chart} at ${text} text: too tall for the 1200×${height} card`, async ({ browser, baseURL }) => {
      const context = await browser.newContext({
        viewport: { width: (SOCIAL.width * 3) / 5, height: (height * 3) / 5 }, // the card in CSS px (scale 5/3)
        colorScheme: "light",
        reducedMotion: "reduce",
      });
      const page = await context.newPage();
      await page.goto(new URL(path, baseURL).href);
      await page.addStyleTag({ content: `html { font-size: ${text}; }` });
      const error = await layoutSocialCard(page, chart, "news.karthik.nz/x").catch((e: unknown) => e);
      expect(error).toBeInstanceOf(TooTallError);
      expect((error as Error).message).toMatch(
        new RegExp(`^chart "${chart}" is \\d+px tall; the card has room for \\d+px$`),
      );
      await context.close();
    });
  }
});
