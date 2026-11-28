# SegmentedBar — test sign-off

Status: **pass, signed off** (light test scope)

Tester: Opus 5.5 component tester (not the builder) · Builder: Opus 5.5 component builder (issue #13, Charts II part 1) · Date: 21 November 2026
Branch / commit: `phase-3/charts/charts-2`, components at 6d17e2a; tests, fixes and this sign-off in the tester commit after it

## All areas

| Area | Result | Tests | Notes |
|---|---|---|---|
| Rendering | ☑ pass | `charts-2.test.ts`; `charts-2.tester.test.ts` | One 28px bar, 2–6 parts (schema), key under the bar in block order with every value |
| Data | ☑ pass | as above | A missing part is "No data" in the key, never a zero; a part below zero is not drawn but stays in the key and table; a very small part keeps its key and value; stops are the drawn parts, one each |
| Content edge cases | ☑ pass | tester spec collision checks | Long part names wrap in the key at 320 and 200% text; nothing overlaps |
| Formatting | ☑ pass | unit tests | "$201.2bn"; table column named by the shared period ("fiscal 2024") |
| Server rendering | ☑ pass | `charts-2-islands.tester.test.tsx` | |
| Design-system compliance | ☑ pass | crimson tests both themes | iPhone (highlighted, positive) crimson; a highlighted part below zero is the neutral highlight in the key |
| Accessibility | ☑ pass | builder axe; tester keyboard | Screen readers: not checked by hand |
| Interaction | ☑ pass after C2-2 | tester spec | Each part is a hover, tap and arrow-key stop; tooltips on screen at every width |
| Themes (light, dark ×2, forced colours, print) | ☑ pass | tester spec | Shares StackedBar.css, so C2-5 and C2-6 fixes apply |
| Motion | ☑ pass | tester spec | |
| Zoom (200% zoom, 200% text) | ☑ pass | tester spec | |
| Failures (images, video, data) | ☑ pass | island scripts blocked | Bar and key read without JS |
| Performance (JS budget, CLS, render time) | ☑ pass | `budgets.test.ts` | No JS of its own (StackedBarPlot) |
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
| Touch (phone, tablet) | ☑ | ☑ | |
| Scale factors 1× / 2× / 3× | ☑ | ☑ | |
| In context (charts article fixture) | ☑ | ☑ | Added as `categorymix` |

## Issues found

| # | Area / screen | Problem | Expected, fix and test |
|---|---|---|---|
| C2-2 | Interaction / 320, Chromium | The tooltip of a part near the middle of the bar ran past the screen's left edge (edge-cases fixture) | **Fixed** in the shared `ChartTooltip`. Test: "every stop's tooltip is wholly on screen" |

## Accepted exceptions and open questions

| Exception or question | Reason | Approved by |
|---|---|---|
| A highlighted part with no value is a crimson "No data" in the key | "A missing value never fell" (charts.md); there is no mark to colour | Noted |
| Light test scope; no screenshot baselines; screen readers not checked by hand | As in the Waterfall sign-off | Karthik (standing); shared |

## Test files

- `site/tests/unit/charts-2.tester.test.ts`, `site/tests/component/charts-2-islands.tester.test.tsx`, `site/tests/e2e/components/charts-2.tester.spec.ts` (tester)
- `site/tests/unit/charts-2.test.ts`, `site/tests/e2e/components/charts-2.spec.ts` (builder)

## Runs

Shared with the other Charts II components; counts in the Sankey sign-off. No SegmentedBar test failed after C2-2.

## Sign-off

☑ Signed (pass): Opus 5.5 component tester, 21 November 2026

## Screenshot baselines (#71)

Accepted 28 Nov 2026 by an Opus component tester that did not build this component. First baselines for the `SegmentedBar/default` and `SegmentedBar/edge-cases` fixture pages (`<main>` after every visible island has hydrated): `site/tests/e2e/components/charts-visual.spec.ts`, PNGs in `site/tests/e2e/__screenshots__/chromium-<screen>/components/charts-visual.spec.ts/`. Made in `mcr.microsoft.com/playwright:v1.65.1-noble` with `bun run test:visual:update`; Chromium, light and dark at 360 and 1440, light at 768 and 980 (site/TESTING.md, "Screenshots", says why not the whole matrix). Reason for accepting: no baseline existed; the images were reviewed against design/charts.md (one highlight, no crimson on a fall, labels whole, nothing past the column) and a second run in the image matched every one pixel for pixel. CI compares the 360 and 1440 ones on every PR (`Site screenshots (smoke, Playwright image)`), nightly all of them. This replaces the "no screenshot baselines" exception above for these screens; Firefox, WebKit and the other widths stay without pixel baselines.
