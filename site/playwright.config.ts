import { defineConfig, devices, type Project } from "@playwright/test";
import type { NewsOptions } from "./tests/e2e/fixtures";
import { FIXTURE_SETS, setPort } from "./tests/e2e/ssr-servers";

// The screen matrix from .claude/skills/news-component-testing/references/playwright-matrix.md and
// Development Plan 3.4. Project names say engine, theme and screen, so `--project` globs pick slices:
//   smoke-*                      CI on every PR: Chromium, 360 and 1440, light and dark, tests tagged @smoke
//   chromium-* firefox-* webkit-*  the full matrix (main / nightly / before a component sign-off)
// Tags decide which projects run a test: untagged tests run on every screen; @smoke also runs in smoke-*;
// @print, @motion and @forced-colors run only in their own projects. See site/TESTING.md.

const WIDTHS = [320, 360, 390, 430, 600, 768, 980, 1120, 1440, 1920] as const;
const THEMES = ["light", "dark"] as const;
const ENGINES = [
  { name: "chromium", device: devices["Desktop Chrome"] },
  { name: "firefox", device: devices["Desktop Firefox"] },
  { name: "webkit", device: devices["Desktop Safari"] },
] as const;

/** Tests tagged with these need special emulation and run only in their own projects. */
const SPECIAL = /@print|@motion|@forced-colors/;

const port = Number(process.env.E2E_PORT ?? 4329);
const baseURL = `http://127.0.0.1:${port}`;

// Phones get 3x, tablets and small laptops 2x, desktops 1x; touch below the desktop breakpoint.
const scaleFor = (width: number) => (width < 600 ? 3 : width < 1120 ? 2 : 1);

const matrix: Project<NewsOptions>[] = ENGINES.flatMap(({ name: engine, device }) => [
  ...THEMES.flatMap((theme) =>
    WIDTHS.map((width) => ({
      name: `${engine}-${theme}-${width}`,
      grepInvert: SPECIAL,
      use: {
        ...device,
        theme,
        colorScheme: theme,
        viewport: { width, height: 900 },
        deviceScaleFactor: scaleFor(width),
        hasTouch: width < 980,
        isMobile: width < 600 && engine !== "firefox", // Firefox has no mobile emulation
      },
    })),
  ),
  // Landscape phone, 844 x 390 (charts and video).
  ...THEMES.map((theme) => ({
    name: `${engine}-${theme}-landscape`,
    grepInvert: SPECIAL,
    use: {
      ...device,
      theme,
      colorScheme: theme,
      viewport: { width: 844, height: 390 },
      deviceScaleFactor: 3,
      hasTouch: true,
      isMobile: engine !== "firefox",
    },
  })),
  {
    name: `${engine}-print`,
    grep: /@print/,
    use: { ...device, theme: "light", colorScheme: "light", viewport: { width: 1120, height: 900 } },
  },
  {
    name: `${engine}-motion`,
    grep: /@motion/,
    use: {
      ...device,
      theme: "light",
      colorScheme: "light",
      viewport: { width: 1440, height: 900 },
      reducedMotion: "no-preference",
    },
  },
]);

const special: Project<NewsOptions>[] = THEMES.map((theme) => ({
  // Forced colours (Windows high contrast) is emulated by Chromium only.
  name: `chromium-forced-colors-${theme}`,
  grep: /@forced-colors/,
  use: {
    ...devices["Desktop Chrome"],
    theme,
    colorScheme: theme,
    forcedColors: "active",
    viewport: { width: 1440, height: 900 },
  },
}));

const smoke: Project<NewsOptions>[] = THEMES.flatMap((theme) =>
  [360, 1440].map((width) => ({
    name: `smoke-${theme}-${width}`,
    grep: /@smoke/,
    grepInvert: SPECIAL,
    // The same screen as chromium-<theme>-<width>, so both compare against one set of baselines.
    snapshotPathTemplate: `{testDir}/__screenshots__/chromium-${theme}-${width}/{testFilePath}/{arg}{ext}`,
    use: {
      ...devices["Desktop Chrome"],
      theme,
      colorScheme: theme,
      viewport: { width, height: 900 },
      deviceScaleFactor: scaleFor(width),
      hasTouch: width < 980,
      isMobile: width < 600,
    },
  })),
);

// Tests always run against the built Worker served by `wrangler dev` (what readers get; `astro preview` hangs
// on every request with the Cloudflare adapter), on a port of its own so a dev server on 4321 is never picked up by mistake. E2E_SKIP_BUILD=1 reuses an existing dist/
// (CI builds once for integration and e2e); E2E_REUSE_SERVER=1 reuses a preview you already started.
const preview = `bunx wrangler dev -c dist/server/wrangler.json --local --ip 127.0.0.1 --port ${port}`;
// The same built Worker once more per other fixture curation (withdrawn lead, region, empty Latest), each with its own
// local state, so the home and region templates are tested in the browser on every set (tests/e2e/ssr-servers.ts).
// Started after the first server, whose command builds dist/.
const setServers = FIXTURE_SETS.map((set) => ({
  command: `bunx wrangler dev -c dist/server/wrangler.json --local --ip 127.0.0.1 --port ${setPort(set)} --var NEWS_FIXTURE_SET:${set} --persist-to .wrangler/state-${set}`,
  url: `http://127.0.0.1:${setPort(set)}`,
  reuseExistingServer: process.env.E2E_REUSE_SERVER === "1",
  timeout: 180_000,
  env: { CLOUDFLARE_ENV: "local" },
  stdout: "ignore" as const,
  stderr: "pipe" as const,
}));

export default defineConfig<NewsOptions>({
  testDir: "./tests/e2e",
  fullyParallel: true,
  // WebKit on Windows runs axe at 3x scale in ~15 s on an idle laptop; 30 s timed out under load in the full matrix.
  timeout: 60_000,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  workers: 2, // the laptop has 4 cores and heavy runs go one at a time; CI runners have 2-4
  reporter: [[process.env.CI ? "github" : "list"], ["html", { open: "never" }]],
  // Baselines have no platform suffix: they are made and compared only on Linux in the official
  // Playwright image (see TESTING.md, "Screenshots"), never on Windows or macOS.
  snapshotPathTemplate: "{testDir}/__screenshots__/{projectName}/{testFilePath}/{arg}{ext}",
  expect: {
    toHaveScreenshot: { maxDiffPixelRatio: 0, animations: "disabled", caret: "hide", scale: "device" },
  },
  use: {
    baseURL,
    reducedMotion: "reduce",
    locale: "en-GB",
    timezoneId: "Europe/London",
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
  },
  webServer: [
    {
      command: process.env.E2E_SKIP_BUILD ? preview : `bun run build && ${preview}`,
      url: baseURL,
      reuseExistingServer: process.env.E2E_REUSE_SERVER === "1",
      timeout: 180_000,
      // Fixture pages (src/pages/fixtures/) are built only with NEWS_FIXTURES=1; e2e always tests them.
      // NEWS_PREVIEW=1 adds the local preview route that tests/e2e/ssr-rest.spec.ts uses; production builds never set it.
      env: { NEWS_FIXTURES: "1", NEWS_PREVIEW: "1", CLOUDFLARE_ENV: "local", OPENUI_TELEMETRY_DISABLED: "1" },
      stdout: "ignore",
      stderr: "pipe",
    },
    ...setServers,
  ],
  projects: [...smoke, ...matrix, ...special],
});
