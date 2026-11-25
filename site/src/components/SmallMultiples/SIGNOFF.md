# SmallMultiples — test sign-off

Status: **signed off** (22 November 2026)

Tester: Opus 5.5 component tester (not the builder) · Builder: Opus 5.5 component builder (issue #13, Charts III part 1) · Date: 22 November 2026
Branch / commit: `phase-3/charts/charts-3`, built at 44f4556; tests and fixes in 378da01 and ebd03b2

Fixtures: `/fixtures/components/SmallMultiples/default/`, `/fixtures/components/SmallMultiples/edge-cases/`; in context in `/fixtures/renderer/charts/` (`marginpanels`).

## All areas

| Area | Result | Tests | Notes |
|---|---|---|---|
| Rendering | ☑ pass | `tests/unit/charts-3.tester.test.ts`, `tests/unit/charts-3.test.ts` | Line and column forms; highlight, none, and a highlight that was left out; panel names; one figure, one table, one CSV |
| Data | ☑ pass | `tests/unit/charts-3.tester.test.ts` ("shared scales"); `tests/unit/data-charts.test.ts` | One value axis from every panel's values and one x axis from every panel's periods, in both forms, with negatives, zeros, a short large series, an all-null series and a dropped dollar series. A period a panel lacks is a gap or a hatched column, never zero. Every number cited (`data-ref`), CSV equals the table |
| Content edge cases | ☑ pass | fixtures; `tests/e2e/components/charts-3.tester.spec.ts` | 2 to 8 panels, long names (wrap), unknown series (No data), another unit (left out), a panel with no values (No data, fixed: issue 1) |
| Formatting | ☑ pass | `tests/unit/charts-3.tester.test.ts`; `tests/unit/data-charts.test.ts` | True minus on the shared axis; units in the subtitle |
| Server rendering | ☑ pass | `tests/component/charts-3-islands.tester.test.tsx`; `tests/ssr/` | Every panel hydrates its server markup with no warning; plot is a tab stop only once hydrated; reads with island scripts blocked (browser test) |
| Design-system compliance | ☑ pass | token lint; `tests/e2e/components/charts-3.tester.spec.ts` (crimson) | Tokens only. Crimson on the highlighted panel that rose; the neutral highlight on one that fell (line rule; a column panel with a column below zero), in both themes |
| Accessibility | ☑ pass | `tests/e2e/components/charts-3.spec.ts`, `charts-3.tester.spec.ts` | axe WCAG 2.2 AA zero violations in both themes and forced colours; one tab stop per panel; arrows, Home, End, Escape; live region. Screen readers checked: none by hand (none on this machine); names and roles asserted |
| Interaction | ☑ pass | `tests/component/charts-3-islands.tester.test.tsx`; `charts-3.tester.spec.ts` | Keys reach every stop; tooltips on screen at every stop at every width; taps on touch screens |
| Themes (light, dark ×2, forced colours, print) | ☑ pass | `charts-3.tester.spec.ts` | Light, dark by system and by `data-theme`; forced colours; print keeps marks |
| Motion | ☑ pass | `charts-3.tester.spec.ts` | Nothing animates on load; ≤ 200 ms with motion allowed |
| Zoom (200% zoom, 200% text) | ☑ pass | `charts-3.tester.spec.ts` | No collisions at 200% text after fixes 2 and 3 |
| Failures (images, video, data) | ☑ pass | `charts-3.tester.spec.ts` | Island scripts blocked: marks, values and the data table still there, no dead tab stop |
| Performance (JS budget, CLS, render time) | ☑ pass | `tests/integration/budgets.test.ts`; `charts-3.tester.spec.ts` | Reuses LineChartPlot (1.6 KB gzip own chunk) and ColumnChartPlot (1.2 KB); page within 80 KB. Layout shift after fonts < 0.001 |
| Cross-browser (Chromium, Firefox, WebKit) | ☑ pass | projects `chromium-*`, `firefox-*`, `webkit-*` | See Runs |

## Different screens

Chromium ran every project below (both charts-3 specs). Firefox and WebKit ran 360 light and 1440 dark, 200% text and zoom included.

| Width | Light | Dark | Notes |
|---|---|---|---|
| 320 | ☑ | ☑ | 400% case for zoom |
| 360 | ☑ | ☑ | Chromium, Firefox and WebKit (light) |
| 390 | ☑ | ☑ | |
| 430 | ☑ | ☑ | |
| 600 | ☑ | ☑ | |
| 768 | ☑ | ☑ | |
| 980 | ☑ | ☑ | |
| 1120 | ☑ | ☑ | |
| 1440 | ☑ | ☑ | Chromium, Firefox and WebKit (dark). 2 panels, not tight: x labels collided before fix 2 |
| 1920 | ☑ | ☑ | |
| Landscape phone 844×390 | ☑ | ☑ | |
| Touch (phone, tablet) | ☑ | ☑ | Taps open and close tooltips |
| Scale factors 1× / 2× / 3× | ☑ | ☑ | By project (3× under 600, 2× under 1120) |
| In context (article, home fixtures) | ☑ | ☑ | Charts article fixture: reads in order, ≥ 16px apart, axe clean in both themes |

## Issues found

| # | Area / screen | Problem | Fix (commit) |
|---|---|---|---|
| 1 | Content edge cases, every screen | A panel whose series has no values drew an empty plot (and a tab stop of six empty stops) with no "No data" | The panel says "No data" under its name and draws no plot (378da01) |
| 2 | Content / zoom, 600 px and up, both themes | Two panels across (not tight) used the full-width x-label plan: "fiscal 2020fiscal 2021…" collided in line and column panels | Panels thin x labels as on phones at every width (378da01) |
| 3 | Zoom, every width, 200% text | The top tick ("80%") ran into the panel name: the plot reserved 20px above itself for a rem-sized label. Shared with every chart | `.chart-plot__frame` room and `.chart-plot__x` height grow with the text size; unchanged at 100% (378da01) |

## Accepted exceptions

| Exception | Reason | Approved by |
|---|---|---|
| None | | |

## Runs

- `bun run test`: 1,721 Vitest tests and 143 SSR tests pass. `bun run test:integration`: site 475/475 (the worker-api and worker-mcp suites need DynamoDB local from `infra:up`, not running; untouched here).
- Chromium, 26 projects (10 widths × 2 themes, landscape ×2, forced colours ×2, print, motion), both charts-3 specs: 1,476 tests, 1,408 passed, 66 skipped as expected (200% browser zoom below 640px, touch-only and hover-only tests), 2 failed (Histogram, see its sign-off) before the exception was recorded.
- Firefox 360 light and 1440 dark: 125 passed, 7 skipped. WebKit 360 light and 1440 dark: 125 passed, 7 skipped.
- Regressions (LineChart, ColumnChart and Charts II specs at 360 and 1440, Chromium): 492 passed; 4 in-context tests failed on a test bug from #52 (the helper scrolled to the Sankey's hidden half), fixed in ebd03b2 and passing.

## Sign-off

☑ Every area and every screen above is passed or has an accepted exception.
Signed: Opus 5.5 component tester, 22 November 2026

## Re-test: x labels chosen by the plot's em width (22 November 2026)

The phone/wide x-label switch is now `@container chart-x (min-width: 40em)` instead of the 600px media query. At 100% text the labels are the same as before at every matrix width; at 200% text a wide screen keeps the phone set. Details, runs and the one open exception (ColumnChart `long` edge fixture, 2.9px gap at 572-581px) are in `ChartFigure/SIGNOFF.md`; tests in `site/tests/e2e/components/x-label-plan.tester.spec.ts`.
