// Page (the root of every page.oui) in context, on every screen (news-component-testing): the renderer's fixture
// article pages, where the primitives sit together. Spec: design/layout.md ("Article page": blocks 28px apart).
// There is no fixture home page yet; the home page is curated data (CLAUDE.md), tested when its components exist.
import { expect, expectNoSidewaysScroll, gotoPage, setTextSize, test } from "../fixtures";
import { expectInsideViewport, expectNoClippedText } from "./primitives";

const PAGES = ["/fixtures/renderer/apple-fy2024/", "/fixtures/renderer/edge-cases/", "/fixtures/renderer/static/"];

test.describe("Page", () => {
  for (const path of PAGES) {
    test(`${path}: one page body, blocks in reading order 28px apart, all inside the screen`, async ({ page }) => {
      await gotoPage(page, path);
      const body = page.locator("main .page-body");
      await expect(body).toHaveCount(1);
      const gaps = await body.evaluate((el) => {
        const kids = [...el.children].map((c) => c.getBoundingClientRect());
        return kids.slice(1).map((r, i) => r.top - (kids[i] as DOMRect).bottom);
      });
      expect(gaps.length).toBeGreaterThan(0);
      for (const g of gaps) expect(g).toBeCloseTo(28, 0);
      const flow = await body.evaluate((el) => getComputedStyle(el).flexDirection);
      expect(flow).toBe("column");
      await expectNoSidewaysScroll(page);
      await expectInsideViewport(page);
      await expectNoClippedText(page);
    });

    test(`${path}: keeps its rhythm at 200% text`, { tag: "@zoom" }, async ({ page }) => {
      await gotoPage(page, path);
      await setTextSize(page, 200);
      await expectNoSidewaysScroll(page);
      await expectInsideViewport(page);
      const gaps = await page.locator("main .page-body").evaluate((el) => {
        const kids = [...el.children].map((c) => c.getBoundingClientRect());
        return kids.slice(1).map((r, i) => r.top - (kids[i] as DOMRect).bottom);
      });
      for (const g of gaps) expect(g, "the gap is a fixed 28px token").toBeCloseTo(28, 0);
    });
  }
});
