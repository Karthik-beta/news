# Playwright matrix (starting point)

Adapt paths once `site/` exists. The full matrix is for local runs; CI uses `--project=smoke-*`.

```ts
// site/playwright.config.ts
import { defineConfig, devices } from "@playwright/test";

const widths = [320, 360, 390, 430, 600, 768, 980, 1120, 1440, 1920];
const themes = ["light", "dark"] as const;
const engines = [
  { name: "chromium", use: devices["Desktop Chrome"] },
  { name: "firefox", use: devices["Desktop Firefox"] },
  { name: "webkit", use: devices["Desktop Safari"] },
];

const matrix = engines.flatMap(engine =>
  themes.flatMap(theme =>
    widths.map(width => ({
      name: `${engine.name}-${theme}-${width}`,
      use: {
        ...engine.use,
        viewport: { width, height: 900 },
        colorScheme: theme,
        deviceScaleFactor: width < 600 ? 3 : width < 1120 ? 2 : 1,
        hasTouch: width < 980,
        isMobile: width < 600 && engine.name !== "firefox", // Firefox has no mobile emulation
      },
      grep: /@visual|@a11y|@interaction/,
    })),
  ),
);

export default defineConfig({
  testDir: "./tests",
  fullyParallel: false,          // laptop: 4 cores; heavy runs go one at a time
  workers: 2,
  reporter: [["list"], ["html", { open: "never" }]],
  expect: { toHaveScreenshot: { maxDiffPixelRatio: 0, animations: "disabled", caret: "hide" } },
  use: { baseURL: "http://localhost:4321", reducedMotion: "reduce" },
  projects: [
    ...matrix,
    { name: "landscape-phone", use: { ...devices["iPhone 14 landscape"] }, grep: /@visual/ },
    { name: "forced-colors", use: { ...devices["Desktop Edge"], forcedColors: "active" }, grep: /@visual|@a11y/ },
    { name: "print", use: { ...devices["Desktop Chrome"] }, grep: /@print/ },
    // CI smoke subset
    ...["light", "dark"].flatMap(theme => [360, 1440].map(width => ({
      name: `smoke-${theme}-${width}`,
      use: { ...devices["Desktop Chrome"], viewport: { width, height: 900 }, colorScheme: theme as "light" | "dark" },
      grep: /@visual|@a11y/,
    }))),
  ],
});
```

## Test file pattern

```ts
// site/tests/components/KeyFigures.spec.ts
import { test, expect } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";

const fixtures = ["default", "long-labels", "negatives", "missing-values", "max-items"];

for (const f of fixtures) {
  test(`KeyFigures ${f} @visual`, async ({ page }) => {
    await page.goto(`/__fixtures/KeyFigures/${f}`);
    await page.evaluate(() => document.fonts.ready);
    await expect(page.locator("[data-component=KeyFigures]")).toHaveScreenshot(`${f}.png`);
    const scroll = await page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth);
    expect(scroll, "no sideways page scroll").toBe(false);
  });

  test(`KeyFigures ${f} @a11y`, async ({ page }) => {
    await page.goto(`/__fixtures/KeyFigures/${f}`);
    const { violations } = await new AxeBuilder({ page }).analyze();
    expect(violations.filter(v => ["serious", "critical"].includes(v.impact ?? ""))).toEqual([]);
  });
}

test("KeyFigures source popover @interaction", async ({ page }) => {
  await page.goto("/__fixtures/KeyFigures/default");
  await page.keyboard.press("Tab");
  await expect(page.getByRole("dialog", { name: /source/i })).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog")).toBeHidden();
});
```

Dark mode via explicit attribute (in addition to `colorScheme`):

```ts
await page.evaluate(() => document.documentElement.setAttribute("data-theme", "dark"));
```

200% zoom check:

```ts
await page.setViewportSize({ width: 640, height: 900 });       // 1280 at 200%
await page.evaluate(() => (document.body.style.zoom = "2"));
```
