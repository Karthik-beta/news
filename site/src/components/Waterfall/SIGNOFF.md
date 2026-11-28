# Waterfall — test sign-off

Status: **pass, signed off** (light test scope, see Runs)

Tester: Opus 5.5 component tester (not the builder) · Builder: Opus 5.5 component builder (issue #13, Charts II part 1) · Date: 21 November 2026
Branch / commit: `phase-3/charts/charts-2`, components at 6d17e2a; tests, fixes and this sign-off in the tester commit after it

## Data check (Apple fiscal 2024, against SEC XBRL)

Every statement-of-operations figure the builder typed into `bundles/charts/data.json` was compared with SEC's companyfacts
(`data.sec.gov/api/xbrl/companyfacts/CIK0000320193.json`, 10-K accession 0000320193-24-000123, 1 Oct 2023 – 28 Sep 2024):
total net sales 391,035; cost of sales 210,352; gross margin 180,683; R&D 31,370; SG&A 26,097; operating income 123,216;
other income/(expense), net 269; income before taxes 123,485; tax 29,749; net income 93,736 (US$ m). **All match**, and every
locator (accession, us-gaap tag, period) is right. The source's URL and published date match the filing. The dimensioned
lines (products 294,866, services, the four product categories, five regions) are not in companyfacts; they add up exactly
to SEC's total net sales. No fixes were needed. The SEC facts are recorded by code in `tests/support/sec/aapl-fy2024-companyfacts.json`
and checked by `charts-2.tester.test.ts`.

## All areas

| Area | Result | Tests | Notes |
|---|---|---|---|
| Rendering | ☑ pass | `charts-2.test.ts`; `charts-2.tester.test.ts` "Waterfall edge cases" | Totals from zero (comparison), changes floating (neutral), result highlighted; labels carry the effect's sign |
| Data | ☑ pass | `charts-2.tester.test.ts` | A missing line hatches every line up to the next total, with its value; a missing first total; a mismatch names every total that does not add up ("… Gross profit or Operating profit …"); exact sums at ≥ 1e12 and 1e-8; another unit or scale left out, never converted; table keeps every line |
| Content edge cases | ☑ pass | `charts-2.tester.spec.ts` "nothing clipped, off screen or overlapping" | 16 rows (schema max), long line names ("Income before provision for income taxes") at 320 |
| Formatting | ☑ pass | `charts-2.test.ts`, `charts-2.tester.test.ts` | "−$210.4bn", "+$269m", true minus, "$1.0bn" |
| Server rendering | ☑ pass | `charts-2-islands.tester.test.tsx`; `tests/ssr` | Hydrates with no warnings; no tab stop before hydration; reuses BarChartPlot (no new JS) |
| Design-system compliance | ☑ pass | `chart-colours.test.ts`; `charts-2.tester.spec.ts` crimson tests | A loss is the neutral highlight in both themes (loss and mismatch fixtures); a profit is crimson; break-even (0) stays crimson |
| Accessibility | ☑ pass | `charts-2.spec.ts` axe light + dark; tester keyboard tests | axe clean everywhere it ran; one tab stop; named by every line and value. Screen readers: not checked by hand (none on this machine) |
| Interaction | ☑ pass after C2-2 | `charts-2.tester.spec.ts` "every stop's tooltip is wholly on screen", taps | Every stop's tooltip on screen at every width |
| Themes (light, dark ×2, forced colours, print) | ☑ pass | tester spec forced colours (light, dark), print | |
| Motion | ☑ pass | tester spec motion | Nothing animates on load; ≤ 200 ms when allowed |
| Zoom (200% zoom, 200% text) | ☑ pass | tester spec "@zoom" | No sideways scroll, no overlap |
| Failures (images, video, data) | ☑ pass | "charts still read when their island scripts fail to load" | Bars, values and table without JS |
| Performance (JS budget, CLS, render time) | ☑ pass | `tests/integration/budgets.test.ts` | No JS of its own (BarChartPlot); page within budget; shift after fonts < 0.001 |
| Cross-browser (Chromium, Firefox, WebKit) | ☑ pass | see Runs | |

## Different screens

| Width | Light | Dark | Notes |
|---|---|---|---|
| 320 | ☑ | ☑ | C2-2 fixed here |
| 360 | ☑ | ☑ | Firefox and WebKit too |
| 390 | ☑ | ☑ | |
| 430 | ☑ | ☑ | |
| 600 | ☑ | ☑ | |
| 768 | ☑ | ☑ | |
| 980 | ☑ | ☑ | |
| 1120 | ☑ | ☑ | |
| 1440 | ☑ | ☑ | Firefox and WebKit too |
| 1920 | ☑ | ☑ | |
| Landscape phone 844×390 | ☑ | ☑ | |
| Touch (phone, tablet) | ☑ | ☑ | Tap opens, tap outside closes |
| Scale factors 1× / 2× / 3× | ☑ | ☑ | Per the matrix config |
| In context (charts article fixture) | ☑ | ☑ | Added as `income` to `/fixtures/renderer/charts/` |

Each width ran once in Chromium in one theme (light 320, 360, 430, 768, 1120; dark 390, 600, 980, 1440, 1920); every colour and
axe test also switches to the other theme by `data-theme` inside the test, so both themes are checked at every width.

## Issues found

| # | Area / screen | Problem | Expected, fix and test |
|---|---|---|---|
| C2-2 | Interaction / 320, Chromium | A floating line's tooltip (Selling, general and administrative) ran 30px past the left edge: alignment by the anchor's place on the track cannot keep a 280px tooltip inside a 180px track | Tooltips stay on screen (design/charts.md, "Interaction"). **Fixed** in the shared `ChartTooltip` (nudged sideways after layout to stay 8px inside the viewport). Test: "every stop's tooltip is wholly on screen" |

## Accepted exceptions and open questions

| Exception | Reason | Approved by |
|---|---|---|
| Light test scope: full widths in Chromium only; Firefox and WebKit at 360 and 1440 | Karthik's standing preference (CPU-limited laptop, another tester in parallel) | Karthik (standing) |
| No screenshot baselines | `@visual` compares only in the Linux Playwright image (site/TESTING.md) | Shared, as for #8 |
| Screen readers not checked by hand | None on this machine; names and the accessibility tree are asserted | Shared, as for #8 |

## Test files

- `site/tests/unit/charts-2.tester.test.ts`, `site/tests/component/charts-2-islands.tester.test.tsx`, `site/tests/e2e/components/charts-2.tester.spec.ts` (tester)
- `site/tests/unit/charts-2.test.ts`, `site/tests/e2e/components/charts-2.spec.ts` (builder); `tests/integration/budgets.test.ts`, `tests/ssr`

## Runs

The six Charts II components ran together; the counts are in the Sankey sign-off's "Runs" section. Chromium: every
width in one theme plus landscape, print, motion and forced colours (1,368 runs). Firefox and WebKit: 360 and 1440 with
200% text and zoom (392 runs; failures there are C2-4, Slope, only). Vitest 1,409, Bun SSR 143, integration 379: all passed.
No Waterfall test failed after C2-2.

## Sign-off

☑ Signed (pass): Opus 5.5 component tester, 21 November 2026

## Screenshot baselines (#71)

Accepted 28 Nov 2026 by an Opus component tester that did not build this component. First baselines for the `Waterfall/default` and `Waterfall/edge-cases` fixture pages (`<main>` after every visible island has hydrated): `site/tests/e2e/components/charts-visual.spec.ts`, PNGs in `site/tests/e2e/__screenshots__/chromium-<screen>/components/charts-visual.spec.ts/`. Made in `mcr.microsoft.com/playwright:v1.65.1-noble` with `bun run test:visual:update`; Chromium, light and dark at 360 and 1440, light at 768 and 980 (site/TESTING.md, "Screenshots", says why not the whole matrix). Reason for accepting: no baseline existed; the images were reviewed against design/charts.md (one highlight, no crimson on a fall, labels whole, nothing past the column) and a second run in the image matched every one pixel for pixel. CI compares the 360 and 1440 ones on every PR (`Site screenshots (smoke, Playwright image)`), nightly all of them. This replaces the "no screenshot baselines" exception above for these screens; Firefox, WebKit and the other widths stay without pixel baselines.
