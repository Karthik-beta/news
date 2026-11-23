// The rest of the SSR site against the built Worker in workerd (what wrangler dev serves): search over local D1, feeds,
// sitemaps, robots.txt and llms.txt, the preview mode and the report form. The Worker is built with NEWS_PREVIEW=1 for
// these tests (playwright.config.ts); a production build has no preview route (tests/integration/ssr-build.test.ts).
import { createHash } from "node:crypto";
import normal from "../../fixtures/data/normal/items.json";
import { expect, expectAccessible, expectNoSidewaysScroll, gotoPage, test } from "./fixtures";

test.describe("search", () => {
  test("finds pages, keeps the filters, and has no violations", { tag: ["@smoke", "@a11y"] }, async ({ page }) => {
    await gotoPage(page, "/search?q=inflation&region=north-america");
    await expect(page.getByRole("heading", { level: 1, name: "Search" })).toBeVisible();
    await expect(page.getByRole("link", { name: "American consumer prices rose again in December" })).toBeVisible();
    await expect(page.getByLabel("Region")).toHaveValue("north-america");
    await expect(page.getByRole("status")).toContainText("page");
    await expect(page.locator('meta[name="robots"]')).toHaveAttribute("content", "noindex, follow");
    await expectNoSidewaysScroll(page);
    await expectAccessible(page);
  });

  test("an empty result says so, with a way on", { tag: "@smoke" }, async ({ page }) => {
    await gotoPage(page, "/search?q=zzzzqqqq");
    await expect(page.getByText(/No pages match/)).toBeVisible();
    await expect(page.getByRole("link", { name: "See the latest pages" })).toBeVisible();
    await expectAccessible(page);
  });

  test("the form searches, and a bad filter is a 400", { tag: "@smoke" }, async ({ page, request }) => {
    await gotoPage(page, "/search");
    await page.getByRole("searchbox", { name: "Search pages" }).fill("steel");
    await page.getByRole("button", { name: "Search" }).click();
    await expect(page).toHaveURL(/\/search\?q=steel/);
    await expect(page.getByRole("link", { name: /steel/i }).first()).toBeVisible();
    expect((await request.get("/search?q=a&region=Not%20A%20Region")).status()).toBe(400);
  });
});

test.describe("machine access", () => {
  test("robots.txt and llms.txt are served", { tag: "@smoke" }, async ({ request }) => {
    const robots = await request.get("/robots.txt");
    expect(await robots.text()).toMatch(
      /Disallow: \/search\n[\s\S]*Sitemap: https:\/\/news\.karthik\.nz\/sitemap\.xml/,
    );
    const llms = await request.get("/llms.txt");
    expect(llms.headers()["content-type"]).toContain("text/plain");
    expect(await llms.text()).toContain("/atom.xml");
  });

  test("feeds, sitemap index and a month's sitemap are XML with the right type", { tag: "@smoke" }, async ({
    request,
  }) => {
    for (const [path, type, root] of [
      ["/rss.xml", "application/rss+xml", "<rss"],
      ["/atom.xml", "application/atom+xml", "<feed"],
      ["/economy/rss.xml", "application/rss+xml", "<rss"],
      ["/sitemap.xml", "application/xml", "<sitemapindex"],
      ["/sitemaps/2025-01.xml", "application/xml", "<urlset"],
    ] as const) {
      const res = await request.get(path);
      expect(res.status(), path).toBe(200);
      expect(res.headers()["content-type"], path).toContain(type);
      expect(await res.text(), path).toContain(root);
    }
    expect((await request.get("/nothing-here/rss.xml")).status()).toBe(404);
  });

  test("a page advertises its feeds, and the footer links to the feeds page", { tag: "@smoke" }, async ({ page }) => {
    await gotoPage(page, "/economy");
    await expect(page.locator('link[rel="alternate"][type="application/rss+xml"]')).toHaveCount(2);
    await expect(page.locator('link[rel="alternate"][href="/economy/atom.xml"]')).toHaveCount(1);
    await page.getByRole("navigation", { name: "About this site" }).getByRole("link", { name: "RSS feeds" }).click();
    await expect(page).toHaveURL(/\/feeds\/$/);
    await expect(page.getByRole("heading", { level: 1 })).toHaveText("RSS and Atom feeds");
  });
});

// The preview server holds one draft in the Worker's memory, so these run one after another.
test.describe("preview mode", () => {
  test.describe.configure({ mode: "serial" });

  /** The fixture curation with another lead: a draft is a curation.v1 body, without the storage fields. */
  const draft = () => {
    const live = (normal as Record<string, unknown>[]).find((i) => i.PK === "HOME#CURATION") as Record<string, never>;
    const { schema, lang, created_at, approved_at, curator_model, slots, checks } = live;
    return JSON.stringify({
      schema,
      lang,
      created_at,
      approved_at,
      curator_model,
      checks,
      slots: { ...(slots as object), lead: { page_id: "japan-wages" } },
    });
  };

  test("a draft changes the next home request with no rebuild, and DELETE brings the live curation back", {
    tag: "@smoke",
  }, async ({ request }) => {
    const live = await request.get("/");
    const liveHtml = await live.text();
    expect(liveHtml).toContain("Apple reports its results for the 2024 fiscal year");
    expect(live.headers()["cache-control"]).toBe("no-store");
    expect(live.headers()["x-robots-tag"]).toBe("noindex");
    expect(live.headers()["x-news-preview"]).toBeUndefined();

    const body = draft();
    const put = await request.put("/__preview/curation", {
      data: body,
      headers: { "content-type": "application/json" },
    });
    expect(put.status()).toBe(204);
    const drafted = await request.get("/");
    const draftedHtml = await drafted.text();
    expect(draftedHtml).not.toBe(liveHtml);
    expect(draftedHtml).toContain("Japan");
    expect(drafted.headers()["x-news-preview"]).toBe(createHash("sha256").update(body).digest("hex"));

    expect((await request.delete("/__preview/curation")).status()).toBe(204);
    const back = await request.get("/");
    expect(await back.text()).toBe(liveHtml);
    expect(back.headers()["x-news-preview"]).toBeUndefined();
  });

  test("a draft that is not curation.v1 is refused and changes nothing", { tag: "@smoke" }, async ({ request }) => {
    const put = await request.put("/__preview/curation", {
      data: "{}",
      headers: { "content-type": "application/json" },
    });
    expect(put.status()).toBe(400);
    expect((await request.get("/")).headers()["x-news-preview"]).toBeUndefined();
  });
});

test.describe("report an error", () => {
  const API = "**/v1/reports";
  const PAGE_ADDRESS = "https://news.karthik.nz/2025/01/us-cpi-december-2024";
  const hydrated = (page: import("@playwright/test").Page) =>
    page.waitForFunction(() => {
      const root = document.querySelector('[data-island="ReportForm"]');
      return root !== null && Object.keys(root).some((k) => k.startsWith("__reactContainer"));
    });

  test("posts JSON to the intake and shows the confirmation", { tag: ["@smoke", "@a11y"] }, async ({ page }) => {
    const posted: unknown[] = [];
    await page.route(API, async (route) => {
      const cors = { "access-control-allow-origin": "*", "access-control-allow-headers": "content-type" };
      if (route.request().method() === "OPTIONS") return route.fulfill({ status: 204, headers: cors });
      posted.push(route.request().postDataJSON());
      return route.fulfill({
        status: 202,
        headers: cors,
        contentType: "application/json",
        body: '{"status":"received"}',
      });
    });
    await gotoPage(page, "/report/?page=us-cpi-december-2024");
    await hydrated(page);
    await expect(page.getByLabel(/Address of the page/)).toHaveValue("us-cpi-december-2024");
    await expectAccessible(page);
    await page.getByLabel(/What is wrong/).fill("The table says 3.1 but the source says 3.2.");
    await page.getByRole("button", { name: "Send report" }).click();
    await expect(page.getByRole("status")).toContainText("Report sent");
    await expect(page.getByRole("status")).toContainText("Your report has been received");
    expect(posted).toEqual([{ page_id: "us-cpi-december-2024", text: "The table says 3.1 but the source says 3.2." }]);
    await expectAccessible(page);
  });

  test("checks the form before sending, takes a pasted address, and shows the intake's refusal", {
    tag: "@smoke",
  }, async ({ page }) => {
    let calls = 0;
    await page.route(API, async (route) => {
      const cors = { "access-control-allow-origin": "*", "access-control-allow-headers": "content-type" };
      if (route.request().method() === "OPTIONS") return route.fulfill({ status: 204, headers: cors });
      calls++;
      return route.fulfill({
        status: 404,
        headers: cors,
        contentType: "application/json",
        body: '{"error":{"code":"unknown_page","message":"x"}}',
      });
    });
    await gotoPage(page, "/report/");
    await hydrated(page);
    await page.getByRole("button", { name: "Send report" }).click();
    await expect(page.getByText("The report was not sent")).toBeVisible();
    expect(calls).toBe(0);
    await expectAccessible(page);
    await page.getByLabel(/Address of the page/).fill(PAGE_ADDRESS);
    await page.getByLabel(/What is wrong/).fill("The table says 3.1 but the source says 3.2.");
    await page.getByRole("button", { name: "Send report" }).click();
    await expect(page.getByText(/No published page has this address/).first()).toBeVisible();
    expect(calls).toBe(1);
  });
});
