# Sankey — test sign-off

Status: **pass, signed off** (light test scope, see Runs)

Tester: Opus 5.5 component tester (not the builder) · Builder: Opus 5.5 component builder (issue #13, Charts II part 1) · Date: 21 November 2026
Branch / commit: `phase-3/charts/charts-2`, components at 6d17e2a; tests, fixes and this sign-off in the tester commit after it

Apple's fiscal 2024 figures were checked against SEC XBRL: all match (see the Waterfall sign-off, "Data check").

## Issue #13 done-when items

| Item | Result | Evidence |
|---|---|---|
| At 360 the Sankey renders the waterfall fallback | ☑ | Measured at every width: the 5-column Apple Sankey shows its waterfall up to 600 (40.9em of the 44em it needs) and its flows from 768 (53.3em, the article measure, the same at 768–1920). At 200% text the plot is at most 26.7em, so the waterfall shows at every width. At 360 every Sankey shows its waterfall, in Chromium, Firefox and WebKit |
| Flow and value sums equal the source rows | ☑ | Builder's sums plus tester's: segments = revenue; each total = carried + additions − costs, exactly; "Other" = sum of its lines; zero lines draw nothing but stay in the table |
| No flow thinner than 2px | ☑ | Layout: ≥ 2 units at 1e12 : 1 and with 11 folded segments. Screen: thinnest flow measured 2.0px (Other income, $269m) at 980–1920, both themes |
| JS within the ADR 0001 budget | ☑ | `SankeyPlot` chunk 0.98 KB gzip of its own (budget 8); charts article page with every Charts II island within 80 KB (`budgets.test.ts`) |
| axe and keyboard | ☑ | axe clean in light and dark at every width run, forced colours too; one tab stop, arrows through all 14 nodes column by column, Home, End, Escape |
| Colour roles in both themes | ☑ | Revenue flows context, costs neutral, profit path crimson; validator tests in `chart-colours.test.ts`; a loss never flows (waterfall with the neutral highlight) |

## All areas

| Area | Result | Tests | Notes |
|---|---|---|---|
| Rendering | ☑ pass | `charts-2.test.ts`; `charts-2.tester.test.ts` "Sankey edge cases" | Segments, additions beside the total, other expense leaving as a cost |
| Data | ☑ pass | as above | Every reason it cannot flow (missing, loss, zero result, totals or segments that do not add up, negative segment, addition after the result) shows the waterfall; folded "Other" writes no number and its tooltip lists each line |
| Content edge cases | ☑ pass | `charts-2.tester.spec.ts` collision checks | No label overlaps another label or another node at any width or at 200% text; 12 long-named segments grow the plot without overlap |
| Formatting | ☑ pass | builder + tester unit tests | "$201.2bn", "$269m" |
| Server rendering | ☑ pass | `charts-2-islands.tester.test.tsx`; `tests/ssr` | Flows, nodes and every label without JS; hydration with no warnings; the hidden half never hydrates |
| Design-system compliance | ☑ pass | crimson tests (both themes) | Profit path crimson only when the result rose; no boxes, no outlines on flows |
| Accessibility | ☑ pass | `charts-2.spec.ts`, tester spec | Screen readers: not checked by hand |
| Interaction | ☑ pass after C2-2 | tester spec and island tests | Hover on the thinnest (2px) flow shows its node; every flow shows its owner's tooltip; tap on a node; tooltips on screen at every stop |
| Themes (light, dark ×2, forced colours, print) | ☑ pass | tester spec | Forced colours: flows GrayText, profit path Highlight, nodes CanvasText |
| Motion | ☑ pass | tester spec | Opacity 120 ms only when motion is allowed |
| Zoom (200% zoom, 200% text) | ☑ pass | tester spec "@zoom" and "switches … at 200% text" | |
| Failures (images, video, data) | ☑ pass | island scripts blocked | Nodes and values still read, no dead tab stop |
| Performance (JS budget, CLS, render time) | ☑ pass | `budgets.test.ts`, tester spec | JS: 0.98 KB gzip own chunk |
| Cross-browser (Chromium, Firefox, WebKit) | ☑ pass | see Runs | |

## Different screens

| Width | Light | Dark | Notes |
|---|---|---|---|
| 320 | ☑ | ☑ | Waterfall; C2-2 fixed here |
| 360 | ☑ | ☑ | Waterfall (all three engines) |
| 390 | ☑ | ☑ | Waterfall |
| 430 | ☑ | ☑ | Apple: waterfall. 3-column edge case: flows (29.5em of 29em), see O-1 |
| 600 | ☑ | ☑ | Apple: waterfall (40.9em of 44em). 3-column: flows |
| 768 | ☑ | ☑ | Flows; labels readable, none overlapping (checked by test and by eye) |
| 980 | ☑ | ☑ | Flows |
| 1120 | ☑ | ☑ | Flows |
| 1440 | ☑ | ☑ | Flows (all three engines) |
| 1920 | ☑ | ☑ | Flows |
| Landscape phone 844×390 | ☑ | ☑ | Flows |
| Touch (phone, tablet) | ☑ | ☑ | Tap opens, tap outside closes |
| Scale factors 1× / 2× / 3× | ☑ | ☑ | |
| In context (charts article fixture) | ☑ | ☑ | Added as `flows` to `/fixtures/renderer/charts/` |

## Issues found

| # | Area / screen | Problem | Expected, fix and test |
|---|---|---|---|
| C2-2 | Interaction / 320, Chromium | The waterfall fallback's tooltips for floating lines ran past the left edge of the screen | **Fixed** in the shared `ChartTooltip`. Test: "every stop's tooltip is wholly on screen" |
| C2-3 | Data / any width | More than 8 columns (possible within 16 rows: segments, 8 totals, a cost after the result) took the 8-column class, so the flows showed at 66.5em though they need up to 81.5em | **Fixed**: classes and container widths for 9 and 10 columns (the schema's maximum); wider never draws flows. Tests: "the widest statement the schema allows (10 columns) …", "the CSS switch-over widths … for 2 to 10 columns" |

## Accepted exceptions and open questions

| Exception or question | Reason | Approved by |
|---|---|---|
| O-1: design/charts.md says the waterfall shows on "every phone"; that holds for the 5-column statement, but a 3-column Sankey (no segments, net sales to gross margin to operating income) needs only 29em and shows its flows on a 430 phone | The width rule is as specified and the 3-column plot reads; the sentence in charts.md is wider than the rule. Either reword it or add a phone floor | Question for Karthik |
| O-3: a cost after the last total leaves from the result node, while the waterfall floats it below the result | Unusual statement shape; both stay truthful | Noted |
| Light test scope; no screenshot baselines; screen readers not checked by hand | As in the Waterfall sign-off | Karthik (standing); shared |

## Test files

- `site/tests/unit/charts-2.tester.test.ts`, `site/tests/component/charts-2-islands.tester.test.tsx`, `site/tests/e2e/components/charts-2.tester.spec.ts` (tester)
- `site/tests/unit/charts-2.test.ts`, `site/tests/e2e/components/charts-2.spec.ts` (builder); `tests/integration/budgets.test.ts`, `tests/ssr`
- `site/tests/support/sec/aapl-fy2024-companyfacts.json`: SEC facts recorded by code for the data check

## Runs (all six Charts II components together)

- **Chromium** (`--workers=1`), tester and builder specs: light 320, 360, 430, 768, 1120; dark 390, 600, 980, 1440, 1920;
  landscape light and dark; print; motion; forced colours light and dark. 1,368 runs: 1,284 passed, 70 skipped (by design:
  200% zoom below 640px, hover-only or touch-only tests, special projects), 14 failed: C2-4 (Slope, 6), C2-5 (StackedBar
  forced colours, 2), and 6 tester test issues: the tap test looked for Sankey nodes where the waterfall shows (5), and
  my "every phone gets the waterfall" assertion at 430, which became question O-1 (1). After the fixes the affected tests re-ran
  on 9 projects: 61 passed, 4 skipped. An earlier run at 768 light (82 passed, 1 skipped) and a stopped first matrix found C2-1 and C2-2.
- **Firefox** light 360 and dark 1440, **WebKit** dark 360 and light 1440, both specs (all but the load-motion and
  blocked-script tests), 200% text and zoom included: 392 runs, 364 passed, 24 skipped, 4 failed (C2-4 only, the same as Chromium).
- **Vitest** (unit, component, astro; `--maxWorkers=1 --pool=forks`): 1,409 passed. **Bun SSR**: 143 passed.
  **Integration** (HTML validity, links, budgets, tokens): 379 passed.
- Not run: Firefox and WebKit at the other widths (light scope). No load-only failures.

## Sign-off

☑ Signed (pass, with O-1 and O-3 open as questions): Opus 5.5 component tester, 21 November 2026
