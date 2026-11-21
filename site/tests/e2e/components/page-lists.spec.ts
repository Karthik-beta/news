// Builder's browser checks for the #11 page types and list components (part 1). Component fixture pages
// (/fixtures/components/<Name>/<fixture>/) and the list parts' galleries (/fixtures/primitives/<Name>/) pass the
// shared checks in both themes; static parts ship no JavaScript; the keyboard reaches pagination in reading
// order; the timeline's dots sit on its line at both layouts. The tester extends this into the full procedure.
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
} from "../fixtures";

const FIXTURES: [string, string][] = [
  ["PolicyDecision", "default"],
  ["PolicyDecision", "edge-cases"],
  ["Finding", "default"],
  ["Finding", "edge-cases"],
  ["Timeline", "default"],
  ["Timeline", "edge-cases"],
];
const PARTS = ["LeadStory", "StoryItem", "SectionHead", "LatestList", "PageContents", "RelatedPages", "Pagination"];
const fixture = (name: string, f: string) => `/fixtures/components/${name}/${f}/`;
const PAGES: [string, string][] = [
  ...FIXTURES.map(([n, f]): [string, string] => [`${n}/${f}`, fixture(n, f)]),
  ...PARTS.map((n): [string, string] => [`${n} gallery`, `/fixtures/primitives/${n}/`]),
];

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

test("static parts ship no JavaScript; the decision's rate path is the only island", { tag: "@smoke" }, async ({
  request,
}) => {
  for (const [n, f] of FIXTURES.filter(([n, f]) => !(n === "PolicyDecision" && f === "default"))) {
    const html = await (await request.get(fixture(n, f))).text();
    expect(html, `${n}/${f}`).not.toContain("<script");
  }
  for (const n of PARTS) {
    const html = await (await request.get(`/fixtures/primitives/${n}/`)).text();
    expect(html.match(/data-island=/g) ?? [], n).toEqual([]);
  }
  const decision = await (await request.get(fixture("PolicyDecision", "default"))).text();
  expect(decision.match(/data-island="([A-Za-z]+)"/g)).toEqual(['data-island="LineChartPlot"']);
});

test("the keyboard reaches Newer, the page numbers and Older in reading order", {
  tag: ["@smoke", "@interaction"],
}, async ({ page }) => {
  await gotoPage(page, "/fixtures/primitives/Pagination/");
  const nav = page.getByRole("navigation", { name: "Pages, long-run case" });
  const links = nav.getByRole("link");
  await expect(links).toHaveText(["Newer", "Page 1", "Page 8", "Page 9", "Page 10", "Page 24", "Older"]);
  await links.first().focus();
  const order: string[] = [];
  for (let i = 0; i < 7; i++) {
    order.push((await page.evaluate(() => document.activeElement?.textContent)) ?? "");
    await page.keyboard.press("Tab");
  }
  expect(order).toEqual(["Newer", "Page 1", "Page 8", "Page 9", "Page 10", "Page 24", "Older"]);
  await expect(nav.locator('[aria-current="page"]')).toHaveText("Page 9");
});

test("timeline dots sit on the line, and the key dot is the only crimson one", { tag: "@smoke" }, async ({ page }) => {
  await gotoPage(page, fixture("Timeline", "default"));
  const geometry = await page.locator(".timeline__event").evaluateAll((events) =>
    events.map((li) => {
      const line = getComputedStyle(li, "::before");
      const dot = getComputedStyle(li, "::after");
      const lineX = Number.parseFloat(line.left) + Number.parseFloat(line.borderLeftWidth) / 2;
      const dotX = Number.parseFloat(dot.left) + Number.parseFloat(dot.width) / 2;
      return { offset: Math.abs(lineX - dotX), dot: dot.backgroundColor };
    }),
  );
  for (const g of geometry) expect(g.offset).toBeLessThanOrEqual(1);
  const colours = new Set(geometry.map((g) => g.dot));
  expect(colours.size).toBe(2);
  expect(geometry.filter((g) => g.dot === geometry[2]?.dot)).toHaveLength(1);
});
