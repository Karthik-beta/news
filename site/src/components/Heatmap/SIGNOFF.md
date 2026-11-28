# Heatmap — test sign-off

Status: **signed off** (22 November 2026)

Tester: Opus 5.5 component tester (not the builder) · Builder: Opus 5.5 component builder (issue #13, Charts III part 1) · Date: 22 November 2026
Branch / commit: `phase-3/charts/charts-3`, built at 44f4556; tests and fixes in 378da01 and ebd03b2

Fixtures: `/fixtures/components/Heatmap/default/`, `/fixtures/components/Heatmap/edge-cases/` (the tester added `wide`, 60 monthly periods); in context in `/fixtures/renderer/charts/` (`marginheat`).

## All areas

| Area | Result | Tests | Notes |
|---|---|---|---|
| Rendering | ☑ pass | `tests/unit/charts-3.tester.test.ts`, `tests/unit/charts-3.test.ts` | Rows, columns, legend, hatched gaps, values when they fit |
| Data | ☑ pass | `tests/unit/charts-3.tester.test.ts` | Every cell's step is the legend range that holds its value, checked exactly on every fixture; ≥ 1e12 and 1e-8 keep their digits; schema 1–12 series; every number cited (`data-ref`) |
| Content edge cases | ☑ pass | fixtures; tester specs | One value (one class, one key: fixed, issue 1), all equal, gaps, all null, unknown, another unit, 60 periods, long names (14ch cap, wrap) |
| Formatting | ☑ pass | `tests/unit/charts-3.tester.test.ts` | True minus in legend, cells and tooltips |
| Server rendering | ☑ pass | `tests/component/charts-3-islands.tester.test.tsx`; `tests/ssr/` | Hydrates with no warning; every name, cell, value and period in the server HTML; reads with scripts blocked |
| Design-system compliance | ☑ pass | `charts-3.tester.spec.ts`; `tests/unit/charts-3.test.ts` (dataviz validator) | Each step is the token the spec names in both themes, by system setting and by `data-theme` either way round (light 300–700; dark 600, 400, 300, 200, 100); every written value at ≥ 4.5:1 on its cell in the browser; never crimson |
| Accessibility | ☑ pass | `charts-3.spec.ts`, `charts-3.tester.spec.ts` | axe zero violations in both themes and forced colours; one tab stop; left/right through cells, up/down a row and stopping at the edges, never scrolling the page; live region names row and period. Screen readers checked: none by hand (none on this machine) |
| Interaction | ☑ pass | `tests/component/charts-3-islands.tester.test.tsx`; `charts-3.tester.spec.ts` | Hover, tap, tap outside; a hatched cell says No data; a touch move opens nothing; tooltips on screen at every stop |
| Themes (light, dark ×2, forced colours, print) | ☑ pass | `charts-3.tester.spec.ts` | Forced colours keep the steps (`forced-color-adjust: none`) with a CanvasText outline; print keeps the colours |
| Motion | ☑ pass | `charts-3.tester.spec.ts` | Opacity transition only, ≤ 200 ms, none under reduced motion |
| Zoom (200% zoom, 200% text) | ☑ pass | `charts-3.tester.spec.ts` | Values shown exactly when the plot is as wide as the server says (100% and 200% text) and always inside their cells; x labels clear after fixes 2 and 3 |
| Failures (images, video, data) | ☑ pass | `charts-3.tester.spec.ts` | Scripts blocked: cells, values and table still there |
| Performance (JS budget, CLS, render time) | ☑ pass | `tests/integration/budgets.test.ts` | HeatmapPlot: 1.8 KB, 0.95 KB gzip own chunk (budget 8 KB); page within 80 KB; no shift after fonts |
| Cross-browser (Chromium, Firefox, WebKit) | ☑ pass | `chromium-*`, `firefox-*`, `webkit-*` | See Runs |

## Different screens

Chromium ran every project below (both charts-3 specs). Firefox and WebKit ran 360 light and 1440 dark, 200% text and zoom included.

| Width | Light | Dark | Notes |
|---|---|---|---|
| 320 | ☑ | ☑ | |
| 360 | ☑ | ☑ | x labels collided before fix 2. 60 periods: cells 1.6px wide between 2px gaps, inside the page and readable only through tooltips and the table (24 periods would be 7.0px; at 1440, 8.1px and 23.4px): open question |
| 390 | ☑ | ☑ | |
| 430 | ☑ | ☑ | |
| 600 | ☑ | ☑ | |
| 768 | ☑ | ☑ | |
| 980 | ☑ | ☑ | |
| 1120 | ☑ | ☑ | |
| 1440 | ☑ | ☑ | 200% text: x labels collided before fix 3 |
| 1920 | ☑ | ☑ | |
| Landscape phone 844×390 | ☑ | ☑ | |
| Touch (phone, tablet) | ☑ | ☑ | |
| Scale factors 1× / 2× / 3× | ☑ | ☑ | |
| In context (article, home fixtures) | ☑ | ☑ | Charts article fixture |

## Issues found

| # | Area / screen | Problem | Fix (commit) |
|---|---|---|---|
| 1 | Content edge cases | One value (42.5) got five classes, "20 to 30" … "60 to 70%": four keys named ranges with no cell | When every value is equal, one class: the range that holds it ("40 to 50%"), the middle step (378da01; `design/charts.md`) |
| 2 | Content, 360 (100% text) | x labels collided ("Jan 2024" × "May 2024"; "Jan 2020" × "Sep 2021"): the label plan ignored the row names, which take about 14ch of the plot | The plan leaves room for the names (378da01) |
| 3 | Zoom, 1440, 200% text | x labels collided: the tight switch measures the whole plot, the axis is narrower by the names | The axis is its own `chart-plot` container, so the shared tight labels switch on by its width (378da01) |
| 4 | Data (signed values) | On signed data (the `matrix` fixture) zero is the middle shade and the most negative row the palest, so falls read as "little" | As decided: `dataShape` and `design/charts.md` say Heatmap is for non-negative magnitudes; signed changes go in a line or column chart (378da01) |

## Accepted exceptions

| Exception | Reason | Approved by |
|---|---|---|
| None | | |

## Open question (Karthik)

Wide heatmaps on phones: with no column cap or scroll in this PR (decided), 60 periods at 360 draw 1.6px cells between 2px gaps, so the grid is mostly paper (24 periods: 7.0px). Nothing breaks (inside the page, every cell a stop, values in tooltips and the table), but it hardly reads. Smallest options: drop the gap when cells would be under about 6px, or tell page writers in `dataShape` to keep heatmaps to 12 periods when phones matter.

## Runs

- `bun run test`: 1,721 Vitest tests and 143 SSR tests pass. `bun run test:integration`: site 475/475.
- Chromium, 26 projects, both charts-3 specs: 1,476 tests, 1,408 passed, 66 skipped as expected; no Heatmap failures after fixes 2 and 3.
- Firefox 360 light and 1440 dark: 125 passed, 7 skipped. WebKit 360 light and 1440 dark: 125 passed, 7 skipped.

## Sign-off

☑ Every area and every screen above is passed or has an accepted exception.
Signed: Opus 5.5 component tester, 22 November 2026

## Re-test: x labels chosen by the plot's em width (22 November 2026)

The phone/wide x-label switch is now `@container chart-x (min-width: 40em)` instead of the 600px media query. At 100% text the labels are the same as before at every matrix width; at 200% text a wide screen keeps the phone set. Details, runs and the one open exception (ColumnChart `long` edge fixture, 2.9px gap at 572-581px) are in `ChartFigure/SIGNOFF.md`; tests in `site/tests/e2e/components/x-label-plan.tester.spec.ts`.

## Screenshot baselines (#71)

Accepted 28 Nov 2026 by an Opus component tester that did not build this component. First baselines for the `Heatmap/default` and `Heatmap/edge-cases` fixture pages (`<main>` after every visible island has hydrated): `site/tests/e2e/components/charts-visual.spec.ts`, PNGs in `site/tests/e2e/__screenshots__/chromium-<screen>/components/charts-visual.spec.ts/`. Made in `mcr.microsoft.com/playwright:v1.65.1-noble` with `bun run test:visual:update`; Chromium, light and dark at 360 and 1440, light at 768 and 980 (site/TESTING.md, "Screenshots", says why not the whole matrix). Reason for accepting: no baseline existed; the images were reviewed against design/charts.md (one highlight, no crimson on a fall, labels whole, nothing past the column) and a second run in the image matched every one pixel for pixel. CI compares the 360 and 1440 ones on every PR (`Site screenshots (smoke, Playwright image)`), nightly all of them. This replaces the "no screenshot baselines" exception above for these screens; Firefox, WebKit and the other widths stay without pixel baselines.
