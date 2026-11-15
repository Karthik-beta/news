# Testing the site

How the site is tested and how to add tests for a component. The procedure a component must pass (every area, every screen, sign-off) is the `news-component-testing` skill; this file is the harness it runs on. Component tests are written and run by an Opus 5.5 tester that did not build the component.

## Levels

Run from `site/` (or the same script from the repo root with `bun --filter @news/site <script>`).

| Level | Command | What runs | Needs |
|---|---|---|---|
| Unit | `bun run test` | Vitest projects `unit` (Node), `component` (React in jsdom + Testing Library + jest-dom), `astro` (`.astro` rendered with the Container API) | nothing; no network. `pretest` syncs `tokens.css` |
| Integration | `bun run test:integration` | builds `dist/`, then Vitest project `integration`: HTML validity (html-validate), internal links and fragments, tokens in the shipped CSS. The checkers themselves are unit-tested on known-bad input (`tests/unit/links.test.ts`, `html-validate-config.test.ts`) | a build (the script does it) |
| E2E smoke | `bun run test:e2e` | Playwright `smoke-*`: Chromium, 360 and 1440, light and dark, tests tagged `@smoke` | Chromium |
| E2E full matrix | `bun run test:e2e:full` | Playwright `chromium-*`, `firefox-*`, `webkit-*`: every screen below (74 projects, 800 runs for the shell today) | all three browsers; CPU-heavy, run alone: about 70 min on the 4-core Windows laptop, WebKit alone about 26 min |
| Screenshots | `bun run test:visual` | `@visual` tests inside the Playwright Docker image | Docker |

Useful while working: `bun run test:watch`; `bunx vitest run --project component`; `bunx playwright test --project='webkit-*' -g "KeyFigures"`; `bunx playwright show-report`.

Browsers: `bunx playwright install chromium firefox webkit` (Linux CI adds `--with-deps`).

E2E always runs against the built site served by `astro preview` on port 4329, never the dev server. Environment switches: `E2E_SKIP_BUILD=1` serves the existing `dist/` (CI builds once), `E2E_REUSE_SERVER=1` reuses a preview you started yourself, `E2E_PORT` changes the port.

## In CI

- Every PR (fast): `bun run lint`, `bun run typecheck`, `bun run test`, `bun run test:integration`, then `bun run test:e2e` with `E2E_SKIP_BUILD=1` after `bunx playwright install --with-deps chromium`. Always pass a project filter: a bare `playwright test` runs the whole matrix.
- `main` / nightly (full): `bunx playwright install --with-deps chromium firefox webkit`, then `bun run test:e2e:full`.
- Screenshots: a job with `container: mcr.microsoft.com/playwright:v<@playwright/test version>-noble`, `NEWS_VISUAL=1`, running `bunx playwright test --grep @visual` (what `bun run test:visual` does locally). Upload `playwright-report/` and `test-results/` as artifacts on failure.
- Cache `~/.bun/install/cache` (key: `bun.lock`) and `~/.cache/ms-playwright` (key: the `@playwright/test` version).

## Screens and tags

Project names are `<engine>-<theme>-<screen>`, so `--project` globs choose a slice (`'*-dark-*'`, `'firefox-*'`, `'*-320'`).

| Projects | Screen |
|---|---|
| `<engine>-<light|dark>-<width>` | widths 320, 360, 390, 430, 600, 768, 980, 1120, 1440, 1920 × 900 high; scale 3× under 600, 2× under 1120, 1× above; touch under 980; mobile emulation under 600 (not Firefox) |
| `<engine>-<light|dark>-landscape` | landscape phone 844 × 390, 3×, touch |
| `<engine>-print` | print media |
| `<engine>-motion` | motion allowed (`prefers-reduced-motion: no-preference`); every other project runs with reduced motion |
| `chromium-forced-colors-<light|dark>` | Windows high contrast (Chromium only) |
| `smoke-<light|dark>-<360|1440>` | CI subset, Chromium |

Engines: `chromium`, `firefox`, `webkit`. Locale `en-GB`, time zone Europe/London.

Tags (Playwright `tag` option) decide where a test runs:

- untagged: every matrix and landscape project, i.e. every screen, both themes, three engines;
- `@smoke`: also in the CI smoke projects; keep it to the few checks that catch a broken page;
- `@a11y`, `@zoom`, `@interaction`, `@visual`: descriptive, runs on every screen; filter with `-g @a11y`;
- `@print`, `@motion`, `@forced-colors`: only in their own projects.

Expected skips, not failures: `@visual` outside the Docker image; CLS outside Chromium (no layout-shift API); the 200% browser-zoom test below 640px wide (the 320 project is the 400% case). The per-test timeout is 60 s because WebKit on Windows runs axe at 3× scale slowly under load.

## Shared checks

Import `test` and `expect` from `tests/e2e/fixtures.ts`, never from `@playwright/test`: its auto fixture fails any test with a console error, an uncaught exception or a failed same-origin request (allow a known one with `test.use({ allowedConsoleErrors: [/…/] })`). It also exports:

`gotoPage` (waits for fonts) · `expectAccessible` (axe, WCAG 2.2 A/AA tags, fails on any violation; `include` scopes it to a component) · `expectNoSidewaysScroll` · `expectNoSmallText` (12px floor) · `expectTargetSize` (24 × 24, inline text links exempt) · `expectDesignFonts` (every visible text uses exactly the serif or sans font token; catches monospace and stray faces) · `setTheme` / `expectTheme` (`data-theme` override and system setting, checked against `tokens.json`) · `setTextSize` (200% text) · `zoomTo200` (200% browser zoom: halves the CSS viewport; skips below 640px, where the 320 project is the 400% case) · `cumulativeLayoutShift` (CLS; Chromium only, `null` elsewhere) · `longestMotionMs` / `runningAnimations`.

Failures (blocked images, offline video, missing data): block with `page.route(glob, (r) => r.abort())` before `gotoPage`, and list the errors the block causes in `test.use({ allowedConsoleErrors: [/…/] })` so nothing else slips through.

For jsdom tests, `tests/support/render.tsx` has `renderComponent(ui, { theme })` (loads `tokens.css`, sets `data-theme`), `cssToken` and `expectNoAxeViolations`. `tests/support/tokens.ts` reads `tokens.json` independently of the generator, so tests compare against the source of truth.

`tests/e2e/shell.spec.ts` is the worked example of every kind of check.

## Adding a component's tests

1. Unit and data edge cases (formatters, data shaping): `src/components/<Name>/<Name>.test.ts` (Node).
2. Rendering, props, keyboard: `src/components/<Name>/<Name>.test.tsx` with `renderComponent` and `user-event`.
3. Server rendering of an `.astro` component: `tests/astro/<Name>.test.ts` with `experimental_AstroContainer` (see `index-page.test.ts`); assert the HTML is complete without JavaScript and ships no script.
4. Fixture pages: one page per fixture under `src/pages/__fixtures/<Name>/` with fixed data and dates; the integration tests then validate their HTML and links automatically.
5. Browser: `tests/e2e/components/<Name>.spec.ts` that visits each fixture and calls the shared checks (untagged, so it runs on every screen), plus `@interaction`, `@print`, `@motion`, `@forced-colors` and `@visual` tests as the component needs, and one `@smoke` test.
6. Run `bun run test`, `bun run test:integration`, `bun run test:e2e`, then the full matrix alone, then `bun run test:visual`. Fill in `TESTING.md` in the component folder from the skill's template.

## Screenshots

Decision: baselines are made and compared only on Linux inside the official image `mcr.microsoft.com/playwright:v<version>-noble` (the version of `@playwright/test` in the lockfile), locally through `bun run test:visual` and in CI by running the job in that same image. Windows and macOS render fonts differently, so local Windows runs never write or compare baselines: `@visual` tests skip unless `NEWS_VISUAL=1`, which only the Docker script and the CI job set. Paths have no platform suffix: `tests/e2e/__screenshots__/<project>/<spec>/<name>.png`, committed.

- Any pixel difference fails (`maxDiffPixelRatio: 0`, animations disabled, caret hidden, device scale kept for line crispness).
- New or changed baselines: `bun run test:visual:update`, then review every changed image in the diff. Accept a baseline only with the reason written in the component's `TESTING.md`.
- Deterministic input: self-hosted fonts, fixed fixture data and dates, reduced motion, fixed viewport and scale, `en-GB` and Europe/London.

Status: the mechanism is in place; the only example (`shell.spec.ts`, "matches its screenshot baseline") is disabled with `test.fixme` until fonts are self-hosted in Phase 3.1, because the shell is a placeholder drawn in fallback system fonts. No baselines are committed yet.

## HTML validation rules

`.htmlvalidate.json` uses `html-validate:recommended` and `html-validate:document`. Style-only rules (`doctype-style`, `void-style`, `attribute-boolean-style`, `attribute-empty-style`, `no-trailing-whitespace`) are off because they judge source formatting and `dist/` is minified output. `require-sri` applies only to cross-origin resources; same-origin assets do not need integrity hashes.
