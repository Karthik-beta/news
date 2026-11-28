# Histogram — test sign-off

Status: **signed off, one open exception awaiting Karthik** (22 November 2026)

Tester: Opus 5.5 component tester (not the builder) · Builder: Opus 5.5 component builder (issue #13, Charts III part 1) · Date: 22 November 2026
Branch / commit: `phase-3/charts/charts-3`, built at 44f4556; tests and fixes in 378da01 and ebd03b2

Fixtures: `/fixtures/components/Histogram/default/`, `/fixtures/components/Histogram/edge-cases/`; in context in `/fixtures/renderer/charts/` (`cpispread`).

## All areas

| Area | Result | Tests | Notes |
|---|---|---|---|
| Rendering | ☑ pass | `tests/unit/charts-3.tester.test.ts`, `tests/unit/charts-3.test.ts` | Ranges, highlight, no highlight, a highlight on a missing or other-unit item (nothing highlighted), the "not counted" note |
| Data | ☑ pass | `tests/unit/charts-3.tester.test.ts` ("bins and counts equal the source rows") | Every fixture histogram recounted from `data.json` with an independent exact decimal comparison: edges one step apart on 1/2/5×10ⁿ, half-open, no empty end range, at most Sturges and 12, every band's tooltip lists only its own range's items with citations. 300 seeded random sets (1–200 values, negatives, duplicates, values on edges, 1e-9 to 1e15) and 18-significant-digit values next to an edge (fixed: issue 1). No count written on the plot |
| Content edge cases | ☑ pass | fixtures; tester specs | One value, all equal, zero, negatives, ≥ 1e12, 1e-8, no data, another unit, every value unknown, 200 items (schema 2–200) |
| Formatting | ☑ pass | `tests/unit/charts-3.tester.test.ts` | True minus in labels and summary; source currency and scale in tooltips; edge labels bare round numbers like any value axis (fixed: issue 2) |
| Server rendering | ☑ pass | `tests/component/charts-3-islands.tester.test.tsx`; `tests/ssr/` | Hydrates with no warning; tab stop only once hydrated; reads with island scripts blocked |
| Design-system compliance | ☑ pass | token lint; `charts-3.tester.spec.ts` (crimson) | Crimson on the highlighted range when the item is not below zero; neutral highlight when it is; both themes |
| Accessibility | ☑ pass | `charts-3.spec.ts`, `charts-3.tester.spec.ts` | axe zero violations in both themes and forced colours; one tab stop; keys; live region. Screen readers checked: none by hand (none on this machine) |
| Interaction | ☑ pass | `tests/component/charts-3-islands.tester.test.tsx`; `charts-3.tester.spec.ts` | A range's tooltip lists at most 8 values and says "N values, 8 listed"; tooltips on screen at every stop; taps |
| Themes (light, dark ×2, forced colours, print) | ☑ pass | `charts-3.tester.spec.ts` | |
| Motion | ☑ pass | `charts-3.tester.spec.ts` | |
| Zoom (200% zoom, 200% text) | ☑ pass, one open exception | `charts-3.tester.spec.ts` (`KNOWN`) | Edge labels inside the plot at every width (first and last pinned). Exception below at 600 px, 200% text |
| Failures (images, video, data) | ☑ pass | `charts-3.tester.spec.ts` | Scripts blocked: columns and table still there |
| Performance (JS budget, CLS, render time) | ☑ pass | `tests/integration/budgets.test.ts` | Reuses ColumnChartPlot (1.2 KB gzip own chunk); page within 80 KB; no shift after fonts |
| Cross-browser (Chromium, Firefox, WebKit) | ☑ pass | `chromium-*`, `firefox-*`, `webkit-*` | See Runs |

## Different screens

Chromium ran every project below (both charts-3 specs). Firefox and WebKit ran 360 light and 1440 dark, 200% text and zoom included.

| Width | Light | Dark | Notes |
|---|---|---|---|
| 320 | ☑ | ☑ | |
| 360 | ☑ | ☑ | Edge labels inside the plot |
| 390 | ☑ | ☑ | |
| 430 | ☑ | ☑ | |
| 600 | ☑ | ☑ | 200% text: open exception (labels crowd) |
| 768 | ☑ | ☑ | |
| 980 | ☑ | ☑ | |
| 1120 | ☑ | ☑ | |
| 1440 | ☑ | ☑ | |
| 1920 | ☑ | ☑ | |
| Landscape phone 844×390 | ☑ | ☑ | |
| Touch (phone, tablet) | ☑ | ☑ | |
| Scale factors 1× / 2× / 3× | ☑ | ☑ | |
| In context (article, home fixtures) | ☑ | ☑ | Charts article fixture |

## Issues found

| # | Area / screen | Problem | Fix (commit) |
|---|---|---|---|
| 1 | Data, all screens | Bins were found with `Number()`: a value with 17+ significant digits next to an edge was rounded into the next range (1.99999999999999999 went in [2, 4)); min and max were picked by float too | Floor in BigInt from the exact decimal; min, max and the table order by exact comparison (378da01) |
| 2 | Formatting, all screens | Edge labels carried a ".0" no edge has: "0.0, 50.0 … 200.0" (US$ bn), "80,000.0" | Bare round numbers when every edge is whole, as on a value axis; half steps keep their decimal ("2.0, 2.5") (378da01) |
| 3 | Zoom, 1440, 200% text | The x labels ran into the "No data, not counted" note: the label row was 24px for a rem-sized label. Shared with every chart | `.chart-plot__x` height grows with the text size (378da01) |

## Accepted exceptions

| Exception | Reason | Approved by |
|---|---|---|
| 600 px, 200% text, edge-cases `large`: "80,000" touches "100,000" and "140,000" touches "160,000" | From 600 px the column plot picks x labels for a full-width axis at any text size and only thins them once the plot is under 20em. The same shared limitation is already open for ColumnChart and LineChart (`tests/e2e/components/data-charts-r1.known.json`). Fix belongs in the shared label plan (choose phone/wide by the plot's width in em rather than the viewport), which touches every chart | Closed 22 November 2026: x labels now switch at 40em of the plot's text; removed from `KNOWN`, the 600 px 200% test passes |

## Runs

- `bun run test`: 1,721 Vitest tests and 143 SSR tests pass. `bun run test:integration`: site 475/475.
- Chromium, 26 projects, both charts-3 specs: 1,476 tests, 1,408 passed, 66 skipped as expected, 2 failed (this exception, light and dark at 600); with the exception recorded, both pass.
- Firefox 360 light and 1440 dark: 125 passed, 7 skipped. WebKit 360 light and 1440 dark: 125 passed, 7 skipped.

## Sign-off

☐ Every area and every screen above is passed or has an accepted exception: one exception awaits Karthik's approval.
Signed: Opus 5.5 component tester, 22 November 2026

## Re-test: x labels chosen by the plot's em width (22 November 2026)

The phone/wide x-label switch is now `@container chart-x (min-width: 40em)` instead of the 600px media query. At 100% text the labels are the same as before at every matrix width; at 200% text a wide screen keeps the phone set. Details, runs and the one open exception (ColumnChart `long` edge fixture, 2.9px gap at 572-581px) are in `ChartFigure/SIGNOFF.md`; tests in `site/tests/e2e/components/x-label-plan.tester.spec.ts`.

## Screenshot baselines (#71)

Accepted 28 Nov 2026 by an Opus component tester that did not build this component. First baselines for the `Histogram/default` and `Histogram/edge-cases` fixture pages (`<main>` after every visible island has hydrated): `site/tests/e2e/components/charts-visual.spec.ts`, PNGs in `site/tests/e2e/__screenshots__/chromium-<screen>/components/charts-visual.spec.ts/`. Made in `mcr.microsoft.com/playwright:v1.65.1-noble` with `bun run test:visual:update`; Chromium, light and dark at 360 and 1440, light at 768 and 980 (site/TESTING.md, "Screenshots", says why not the whole matrix). Reason for accepting: no baseline existed; the images were reviewed against design/charts.md (one highlight, no crimson on a fall, labels whole, nothing past the column) and a second run in the image matched every one pixel for pixel. CI compares the 360 and 1440 ones on every PR (`Site screenshots (smoke, Playwright image)`), nightly all of them. This replaces the "no screenshot baselines" exception above for these screens; Firefox, WebKit and the other widths stay without pixel baselines.
