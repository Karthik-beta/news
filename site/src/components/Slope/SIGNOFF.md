# Slope — test sign-off

Status: **pass: C2-4 fixed and re-tested** (two open design questions for Karthik, O-4 and O-5; neither blocks)

Tester: Opus 5.5 component tester (not the builder) · Builder: Opus 5.5 component builder (issue #13, Charts II part 1) · Date: 21 November 2026
Branch / commit: `phase-3/charts/charts-2`; components at 6d17e2a, C2-4 fix at 04166db; first test round in aac3e23; re-test tests and this sign-off in the tester commit after 04166db

## All areas

| Area | Result | Tests | Notes |
|---|---|---|---|
| Rendering | ☑ pass | `charts-2.test.ts`; `charts-2.tester.test.ts` "Slope" | Values left, values and names right, period names above the ends; highlight drawn last. Tight plot: right-hand values only, a list of names and both values below |
| Data | ☑ pass | as above | A missing end is "No data" level with the other end; another unit left out; 2–12 items (schema). Tight list: every value carries its `data-ref`; "No data" carries none |
| Content edge cases | ☑ pass | tester spec collision and tight-layout checks; unit "tight layout (C2-4)" | 9 crowded items and long names nudged apart with no overlap at every width and at 200% text; 12 near-equal items: one-line tight values stack apart inside the plot, in value order (unit) |
| Formatting | ☑ pass | unit tests | "−2.6%", "$162.6bn"; tight list "Europe $94.3bn to $101.3bn" |
| Server rendering | ☑ pass | `charts-2-islands.tester.test.tsx` | Tight positions and the gutter (`--slope-end-room`) come from the server; no client measuring |
| Design-system compliance | ☑ pass | crimson tests both themes; tight-list colour test both themes | Europe (rose) crimson; Greater China (fell) the neutral highlight in line, dots, values and the tight list, never crimson; beyond 8 items with no highlight, neutral; "No data" in ink 2 |
| Accessibility | ☑ pass | builder axe; tester keyboard; axe at 200% text on the tight plot | The tight list is `aria-hidden`: screen readers get the plot's summary (`role="img"` label) and the data table, both tested present. Screen readers: not checked by hand |
| Interaction | ☑ pass after C2-2 | tester spec | Pointer picks the nearest line; tooltips on screen at every width; taps |
| Themes (light, dark ×2, forced colours, print) | ☑ pass | tester spec; screenshots at 320 / 200% text in light, dark, forced colours and print | Tight layout reads in all four |
| Motion | ☑ pass | tester spec | |
| Zoom (200% zoom, 200% text) | ☑ pass after C2-4 | tester spec "reflows at 200% text size …" and "Slope's tight layout …" | At 200% text every phone plot (≤ 430) is tight: no sideways scroll, right-hand values inside the plot and clear of the lines, one line each, lines keep ≥ 25% of the width, the list matches the right-hand order |
| Failures (images, video, data) | ☑ pass | island scripts blocked | |
| Performance (JS budget, CLS, render time) | ☑ pass | `budgets.test.ts` | Budgets pass with the fix; no layout shift (tester spec) |
| Cross-browser (Chromium, Firefox, WebKit) | ☑ pass | see Runs | Re-test: Chromium every width; Firefox and WebKit at 360 and 1440 |

## Different screens

| Width | Light | Dark | Notes |
|---|---|---|---|
| 320 | ☑ | ☑ | 200% text: tight layout (C2-4 fixed); checked by eye in light, dark, forced colours, print |
| 360 | ☑ | ☑ | As 320, in Chromium, Firefox and WebKit. 100% text pixel-identical to before the fix |
| 390 | ☑ | ☑ | As 320 |
| 430 | ☑ | ☑ | Tight at 200% text |
| 600 | ☑ | ☑ | |
| 768 | ☑ | ☑ | |
| 980 | ☑ | ☑ | |
| 1120 | ☑ | ☑ | |
| 1440 | ☑ | ☑ | Firefox and WebKit too. 100% text pixel-identical to before the fix |
| 1920 | ☑ | ☑ | |
| Landscape phone 844×390 | ☑ | ☑ | |
| Touch (phone, tablet) | ☑ | ☑ | |
| Scale factors 1× / 2× / 3× | ☑ | ☑ | |
| In context (charts article fixture) | ☑ | ☑ | Added as `regionslope` |

No plot is tight at 100% text on any screen (tested at every width).

## Issues found

| # | Area / screen | Problem | Expected, fix and test |
|---|---|---|---|
| C2-4 | Zoom: 200% text / 320, 360, 390, light and dark; Chromium, Firefox, WebKit | The plot's fixed gutters (4.5em + 10em) made the page 408px wide in a 320–390 viewport | **Fixed** (04166db): a tight layout by container query (`@container slope (max-width: 20em)`). Re-tested: the first-round test passes everywhere; new tests in `charts-2.tester.spec.ts` "Slope's tight layout at large text sizes (C2-4 re-test)" and the unit test "tight layout (C2-4)" fail on the pre-fix build (3 of 6 e2e at chromium-light-360) and pass with the fix. **Closed** |
| C2-2 | Interaction / 320 | A centred tooltip (long item names) ran past the screen's left edge | **Fixed** in the shared `ChartTooltip` |

## Accepted exceptions and open questions

| Exception or question | Reason | Approved by |
|---|---|---|
| O-4: in a crowded slope with nothing highlighted (9 items), labels nudged apart can sit a line or two from their dot. In the tight layout the values stand alone, so this reads worse: in `Slope/edge-cases` "crowded" at 320 / 200% text, "2.9%" sits beside the dot that ends at 0.9%, and "0.9%" beside the 0.4% dot | No overlap (tested); the list below and the table hold every value. The shared stacker (`stackLabels`) only pushes labels down; centring each cluster on its dots, or a short leader from each value to its dot, would tie them. A design call, not small | Question for Karthik |
| O-5: in the tight list, an item that wraps onto a second line has the same spacing as the gap to the next item, so "Americas $162.6bn to / $167.0bn / Europe …" runs together at 320 / 200% text | Wording and order are right; a hanging indent or a larger row gap would separate items. A design call | Question for Karthik |
| Arrow keys step through items in drawing order (the highlight last), not top to bottom | Every item is reachable; the tooltip names it | Noted |
| In the tight list the highlight's first value is in ink; only its second value takes its colour | As `design/charts.md` says ("its value in its colour"); in the plot both ends are coloured | Noted |
| Light test scope; no screenshot baselines; screen readers not checked by hand | As in the Waterfall sign-off | Karthik (standing); shared |

## Test files

- `site/tests/unit/charts-2.tester.test.ts`, `site/tests/component/charts-2-islands.tester.test.tsx`, `site/tests/e2e/components/charts-2.tester.spec.ts` (tester)
- `site/tests/unit/charts-2.test.ts`, `site/tests/e2e/components/charts-2.spec.ts` (builder)

## Runs

First round: shared with the other Charts II components; counts in the Sankey sign-off.

C2-4 re-test (head 04166db plus the tester's tests, 1 worker):
- Chromium, every width, landscape, print, motion, forced colours: all Slope tests (builder and tester) 340 passed, 20 skipped (touch-only or width-gated); new tight-layout tests 104 passed, 28 skipped (phone-only checks above 430)
- Firefox and WebKit at 360 and 1440, light and dark: all Slope tests including the new ones 160 passed, 16 skipped
- 100% text unchanged: every Slope plot (6) at 360 and 1440 in light and dark screenshotted on the fixed build and compared with a build of the pre-fix sources: pixel-identical
- Vitest (site): 1959 passed in 43 files; `bun run typecheck`: 0 errors
- No load-flaky failures

## Sign-off

☑ Signed off: C2-4 closed. O-4 and O-5 are open design questions for Karthik, not blockers.
Signed (pass): Opus 5.5 component tester, 21 November 2026

## Screenshot baselines (#71)

Accepted 28 Nov 2026 by an Opus component tester that did not build this component. First baselines for the `Slope/default` and `Slope/edge-cases` fixture pages (`<main>` after every visible island has hydrated): `site/tests/e2e/components/charts-visual.spec.ts`, PNGs in `site/tests/e2e/__screenshots__/chromium-<screen>/components/charts-visual.spec.ts/`. Made in `mcr.microsoft.com/playwright:v1.65.1-noble` with `bun run test:visual:update`; Chromium, light and dark at 360 and 1440, light at 768 and 980 (site/TESTING.md, "Screenshots", says why not the whole matrix). Reason for accepting: no baseline existed; the images were reviewed against design/charts.md (one highlight, no crimson on a fall, labels whole, nothing past the column) and a second run in the image matched every one pixel for pixel. CI compares the 360 and 1440 ones on every PR (`Site screenshots (smoke, Playwright image)`), nightly all of them. This replaces the "no screenshot baselines" exception above for these screens; Firefox, WebKit and the other widths stay without pixel baselines.
