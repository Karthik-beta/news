import { defineConfig, devices } from "@playwright/test";

// Phase 0: smoke projects only. The full component matrix (10 widths x 2 themes x 3 engines)
// is added in Phase 3 from .claude/skills/news-component-testing/references/playwright-matrix.md.
export default defineConfig({
  testDir: "./tests/e2e",
  fullyParallel: false,
  workers: 2,
  reporter: [["list"]],
  use: { baseURL: "http://127.0.0.1:4321", reducedMotion: "reduce" },
  webServer: {
    command: "bun run dev",
    url: "http://127.0.0.1:4321",
    reuseExistingServer: true,
    timeout: 120_000,
  },
  projects: ["light", "dark"].flatMap((theme) =>
    [360, 1440].map((width) => ({
      name: `smoke-${theme}-${width}`,
      use: { ...devices["Desktop Chrome"], viewport: { width, height: 900 }, colorScheme: theme as "light" | "dark" },
    })),
  ),
});
