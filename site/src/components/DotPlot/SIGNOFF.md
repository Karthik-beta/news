# DotPlot — test sign-off

Status: **pass, signed off** (light test scope)

Tester: Opus 5.5 component tester (not the builder) · Builder: Opus 5.5 component builder (issue #13, Charts II part 1) · Date: 21 November 2026
Branch / commit: `phase-3/charts/charts-2`, components at 6d17e2a; tests, fixes and this sign-off in the tester commit after it

Both modes tested: dots (one value per row) and dumbbells (two values).

## All areas

| Area | Result | Tests | Notes |
|---|---|---|---|
| Rendering | ☑ pass | `charts-2.test.ts`; `charts-2.tester.test.ts` "DotPlot" | Sorted by the last value, largest first, missing last; "given" keeps the order; legend (dot keys) only for dumbbells |
| Data | ☑ pass after C2-1 | as above | Smaller value's label left, larger's right, whichever comes first; ties; a missing end is "No data" with no joining line; negative, zero and positive on one scale; single dot; another unit left out |
| Content edge cases | ☑ pass after C2-1 | tester spec collision checks | Long labels at 320–390 wrap above the track; no label overlaps at 100% or 200% text |
| Formatting | ☑ pass | unit tests | Negative values with a true minus through the shared formatter; "$167.0bn"; rates keep the source's digits |
| Server rendering | ☑ pass | `charts-2-islands.tester.test.tsx` | |
| Design-system compliance | ☑ pass | crimson tests both themes | Europe (rose) crimson second dot; Greater China (fell) and "A fall" (below zero) the neutral highlight; no highlight: cat-1 |
| Accessibility | ☑ pass | builder axe; tester keyboard | Screen readers: not checked by hand |
| Interaction | ☑ pass after C2-2 | tester spec | Row stops; tooltips on screen at every width; taps |
| Themes (light, dark ×2, forced colours, print) | ☑ pass | tester spec | Forced colours: dots CanvasText, subject Highlight |
| Motion | ☑ pass | tester spec | |
| Zoom (200% zoom, 200% text) | ☑ pass after C2-1 | tester spec | |
| Failures (images, video, data) | ☑ pass | island scripts blocked | |
| Performance (JS budget, CLS, render time) | ☑ pass | `budgets.test.ts` | JS: 0.87 KB gzip own chunk |
| Cross-browser (Chromium, Firefox, WebKit) | ☑ pass | see Runs | |

## Different screens

| Width | Light | Dark | Notes |
|---|---|---|---|
| 320 | ☑ | ☑ | C2-1 and C2-2 fixed here |
| 360 | ☑ | ☑ | Firefox and WebKit too |
| 390 | ☑ | ☑ | |
| 430 | ☑ | ☑ | |
| 600 | ☑ | ☑ | Labels move to a left column |
| 768 | ☑ | ☑ | |
| 980 | ☑ | ☑ | |
| 1120 | ☑ | ☑ | |
| 1440 | ☑ | ☑ | Firefox and WebKit too |
| 1920 | ☑ | ☑ | |
| Landscape phone 844×390 | ☑ | ☑ | |
| Touch (phone, tablet) | ☑ | ☑ | |
| Scale factors 1× / 2× / 3× | ☑ | ☑ | |
| In context (charts article fixture) | ☑ | ☑ | Added as `regionchange` (dumbbell, Greater China highlighted and fell) |

## Issues found

| # | Area / screen | Problem | Expected, fix and test |
|---|---|---|---|
| C2-1 | Content, zoom / 320 at 200% text, Chromium light | In a dumbbell row with one value missing ("Second not published"), "No data" sat inside the track at its far end and ran into the other dot's value ("0.9%") | Labels never overlap (charts.md, "Labels"). **Fixed**: "No data" sits in the label room past the track's end, the other dot's value goes on its far side, and the label room counts "No data". Tests: unit "a missing end is 'No data' …" (sides and room), e2e collision check at 200% text |
| C2-2 | Interaction / 320 | A row's tooltip ran past the left edge | **Fixed** in the shared `ChartTooltip` |

## Accepted exceptions and open questions

| Exception or question | Reason | Approved by |
|---|---|---|
| Light test scope; no screenshot baselines; screen readers not checked by hand | As in the Waterfall sign-off | Karthik (standing); shared |

## Test files

- `site/tests/unit/charts-2.tester.test.ts`, `site/tests/component/charts-2-islands.tester.test.tsx`, `site/tests/e2e/components/charts-2.tester.spec.ts` (tester)
- `site/tests/unit/charts-2.test.ts`, `site/tests/e2e/components/charts-2.spec.ts` (builder)

## Runs

Shared with the other Charts II components; counts in the Sankey sign-off. No DotPlot test failed after C2-1 and C2-2.

## Sign-off

☑ Signed (pass): Opus 5.5 component tester, 21 November 2026
