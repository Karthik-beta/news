# StackedBar — test sign-off

Status: **pass, signed off** (light test scope; one rule question open, O-2)

Tester: Opus 5.5 component tester (not the builder) · Builder: Opus 5.5 component builder (issue #13, Charts II part 1) · Date: 21 November 2026
Branch / commit: `phase-3/charts/charts-2`, components at 6d17e2a; tests, fixes and this sign-off in the tester commit after it

## All areas

| Area | Result | Tests | Notes |
|---|---|---|---|
| Rendering | ☑ pass | `charts-2.test.ts`; `charts-2.tester.test.ts` "StackedBar and SegmentedBar" | 2–8 parts, 1–12 rows (schema); categorical colours in order, or one highlight and context; legend in part order |
| Data | ☑ pass | as above | A negative part is left out of the bar but kept in the tooltip and table; a gap is "No data"; a row with no parts is hatched; one scale for every row, a published total past the longer of bar and total; another unit left out |
| Content edge cases | ☑ pass | tester spec collision checks | Long row label at 320 and 200% text; no label or total overlaps |
| Formatting | ☑ pass | unit tests | "−5%" in the tooltip, true minus |
| Server rendering | ☑ pass | `charts-2-islands.tester.test.tsx` | Hydration with no warnings |
| Design-system compliance | ☑ pass after C2-5 | crimson tests both themes; `chart-colours.test.ts` | "Europe" (rose) crimson; "Greater China" (fell) the neutral highlight in bar and legend, light and dark |
| Accessibility | ☑ pass | builder axe; tester keyboard | One tab stop, a stop per row. Screen readers: not checked by hand |
| Interaction | ☑ pass | tester spec | Tooltip per row lists every part; on screen at every width; taps |
| Themes (light, dark ×2, forced colours, print) | ☑ pass after C2-5, C2-6 | tester spec | |
| Motion | ☑ pass | tester spec | |
| Zoom (200% zoom, 200% text) | ☑ pass | tester spec | |
| Failures (images, video, data) | ☑ pass | island scripts blocked | |
| Performance (JS budget, CLS, render time) | ☑ pass | `budgets.test.ts` | JS: 0.85 KB gzip own chunk (shared with SegmentedBar) |
| Cross-browser (Chromium, Firefox, WebKit) | ☑ pass | see Runs | |

## Different screens

| Width | Light | Dark | Notes |
|---|---|---|---|
| 320 | ☑ | ☑ | |
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
| In context (charts article fixture) | ☑ | ☑ | Added as `regionmix` |

Each Chromium width ran in one theme; colour and axe tests switch to the other theme inside the test.

## Issues found

| # | Area / screen | Problem | Expected, fix and test |
|---|---|---|---|
| C2-5 | Data, themes / every screen, Chromium (seen in forced colours) | A row with every part missing showed an empty outlined box, not the hatch: `.stacked-bar__segment { background: … }` loads after ChartFigure.css and the shorthand wiped `.chart-hatch`'s background image | A mark with no data is hatched (design/charts.md, "Missing data"). **Fixed**: `background-color` instead of the shorthand, and a transparent ground under the hatch. Tests: "a mark with no data is hatched, never an empty box (C2-5)", forced-colours "keeps every mark visible" |
| C2-6 | Themes: forced colours / 1440, light and dark | A highlighted part that fell was drawn in CanvasText like every other part (only `highlight` mapped to `Highlight`), so the subject was lost | As the bar chart: either highlight role is `Highlight`. **Fixed** in StackedBar.css. Test: "forced colours: a highlighted part that fell is drawn in Highlight …" |
| C2-2 | Interaction / 320 | (shared tooltip; no StackedBar stop failed, fix applies) | See the Waterfall sign-off |

## Accepted exceptions and open questions

| Exception or question | Reason | Approved by |
|---|---|---|
| O-2: the "fell" rule compares the highlighted part's value in the last row with the first row (the line rule). That only means "fell" when rows are periods in time order. With rows that are categories (countries, segments) a highlighted part is drawn neutral just because the last row's value is smaller than the first's | Rule as written in design/charts.md; every fixture's rows are periods. Options: apply the rule only when every row label is a period, or take an explicit `fell`/comparison from the page writer | Question for Karthik |
| Light test scope; no screenshot baselines; screen readers not checked by hand | As in the Waterfall sign-off | Karthik (standing); shared |

## Test files

- `site/tests/unit/charts-2.tester.test.ts`, `site/tests/component/charts-2-islands.tester.test.tsx`, `site/tests/e2e/components/charts-2.tester.spec.ts` (tester)
- `site/tests/unit/charts-2.test.ts`, `site/tests/e2e/components/charts-2.spec.ts` (builder); `tests/integration/budgets.test.ts`

## Runs

Shared with the other Charts II components; counts in the Sankey sign-off. StackedBar failures: C2-5 in forced colours
light and dark (2); after the fix the StackedBar tests re-ran on 9 projects and passed.

## Sign-off

☑ Signed (pass, O-2 open as a question): Opus 5.5 component tester, 21 November 2026
