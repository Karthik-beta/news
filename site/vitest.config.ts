/// <reference types="vitest/config" />
import { getViteConfig } from "astro/config";

// One config, four projects. getViteConfig loads astro.config.mjs (React integration, .astro compiler),
// and every project extends it, so .astro and .tsx files compile the same way as in the real build.
//   unit         plain TypeScript in Node: formatters, data helpers, tokens. No DOM, no network.
//   component    React components in jsdom with Testing Library and jest-dom matchers.
//   astro        .astro pages and components rendered to HTML with the Astro Container API (server rendering).
//   integration  checks on the built site in dist/ (HTML validity, internal links, tokens). Needs `bun run build`.
// `bun run test` runs the first three; `bun run test:integration` builds, then runs the last.
export default getViteConfig({
  test: {
    projects: [
      {
        extends: true,
        test: {
          name: "unit",
          environment: "node",
          include: ["tests/unit/**/*.test.ts", "src/**/*.test.ts"],
        },
      },
      {
        extends: true,
        test: {
          name: "component",
          environment: "jsdom",
          include: ["tests/component/**/*.test.tsx", "src/**/*.test.tsx"],
          setupFiles: ["tests/setup/component.ts"],
        },
      },
      {
        extends: true,
        test: {
          name: "astro",
          environment: "node",
          include: ["tests/astro/**/*.test.ts"],
        },
      },
      {
        extends: true,
        test: {
          name: "integration",
          environment: "node",
          include: ["tests/integration/**/*.test.ts"],
          testTimeout: 30_000,
        },
      },
    ],
  },
});
