// Builder's browser checks for the #11 extension set (src/components/EXTENSIONS.md): SourceCard, Steps and the
// LineChart notes pass the shared checks in both themes; SourceCard and Steps ship no JavaScript; every note number
// shows on the plot at this screen size; and the keyboard goes through the region links in order (the #11 keyboard
// path not covered by the page-lists and controls specs). The tester extends this into the full procedure.
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
  ["SourceCard", "default"],
  ["SourceCard", "edge-cases"],
  ["Steps", "default"],
  ["Steps", "edge-cases"],
  ["LineChart", "notes"],
];
const fixture = (name: string, f: string) => `/fixtures/components/${name}/${f}/`;

for (const [name, f] of FIXTURES) {
  test(`${name}/${f}: accessible in both themes, reads at this screen size, in the design fonts`, {
    tag: ["@smoke", "@a11y"],
  }, async ({ page, theme }) => {
    await gotoPage(page, fixture(name, f));
    await expectAccessible(page);
    await expectNoSidewaysScroll(page);
    await expectNoSmallText(page);
    await expectTargetSize(page);
    await expectDesignFonts(page);
    await setTheme(page, theme === "light" ? "dark" : "light");
    await expectAccessible(page);
  });
}

test("SourceCard and Steps ship no JavaScript", { tag: "@smoke" }, async ({ request }) => {
  for (const [n, f] of FIXTURES.filter(([n]) => n !== "LineChart")) {
    const html = await (await request.get(fixture(n, f))).text();
    expect(html, `${n}/${f}`).not.toContain("<script");
  }
});

test("every note's number shows on the plot, and matches its line in the list", { tag: "@smoke" }, async ({ page }) => {
  await gotoPage(page, fixture("LineChart", "notes"));
  const chart = page.locator("figure").filter({ has: page.locator("#cpi-title") });
  const labels = chart.locator(".line-chart__event-label");
  await expect(labels).toHaveText(["1", "2", "3"]);
  for (const label of await labels.all()) await expect(label).toBeVisible();
  await expect(chart.locator(".line-chart__notes li")).toHaveCount(3);
});

test("the keyboard goes through the region links in order and marks the current one", {
  tag: ["@smoke", "@interaction"],
}, async ({ page, browserName }) => {
  // Playwright's WebKit on Windows never moves to links with Tab (Safari's default; see SourceRef.spec.ts).
  test.skip(browserName === "webkit", "WebKit does not tab to links by default");
  await gotoPage(page, "/fixtures/primitives/RegionFilter/");
  const nav = page.getByRole("navigation", { name: "Regions, North America case" });
  const links = nav.getByRole("link");
  const names = await links.allTextContents();
  expect(names[0]).toBe("All regions");
  await links.first().focus();
  const order: string[] = [];
  for (let i = 0; i < names.length; i++) {
    order.push((await page.evaluate(() => document.activeElement?.textContent)) ?? "");
    await page.keyboard.press("Tab");
  }
  expect(order).toEqual(names);
  await expect(nav.locator('[aria-current="page"]')).toHaveText("North America");
  await nav.getByRole("link", { name: "Europe" }).focus();
  await page.keyboard.press("Enter");
  await expect(page).toHaveURL(/\?region=europe$/);
});
