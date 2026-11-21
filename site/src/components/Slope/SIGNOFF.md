# Slope — test sign-off

Status: **fail: C2-4 open, sent back to the builder** (everything else passes)

Tester: Opus 5.5 component tester (not the builder) · Builder: Opus 5.5 component builder (issue #13, Charts II part 1) · Date: 21 November 2026
Branch / commit: `phase-3/charts/charts-2`, components at 6d17e2a; tests, fixes and this sign-off in the tester commit after it

## All areas

| Area | Result | Tests | Notes |
|---|---|---|---|
| Rendering | ☑ pass | `charts-2.test.ts`; `charts-2.tester.test.ts` "Slope" | Values left, values and names right, period names above the ends; highlight drawn last |
| Data | ☑ pass | as above | A missing end is "No data" level with the other end; another unit left out; 2–12 items (schema) |
| Content edge cases | ☑ pass at 100% text | tester spec collision checks | 9 crowded items and long names nudged apart with no overlap at every width; 12 near-equal items grow the plot (unit) |
| Formatting | ☑ pass | unit tests | "−2.6%", "$162.6bn" |
| Server rendering | ☑ pass | `charts-2-islands.tester.test.tsx` | |
| Design-system compliance | ☑ pass | crimson tests both themes | Europe (rose) crimson; Greater China (fell) the neutral highlight in line, dots and values; beyond 8 items with no highlight, neutral, never a generated colour |
| Accessibility | ☑ pass | builder axe; tester keyboard | Screen readers: not checked by hand |
| Interaction | ☑ pass after C2-2 | tester spec | Pointer picks the nearest line; tooltips on screen at every width; taps |
| Themes (light, dark ×2, forced colours, print) | ☑ pass | tester spec | |
| Motion | ☑ pass | tester spec | |
| Zoom (200% zoom, 200% text) | ☒ fail | tester spec "reflows at 200% text size …" | C2-4 |
| Failures (images, video, data) | ☑ pass | island scripts blocked | |
| Performance (JS budget, CLS, render time) | ☑ pass | `budgets.test.ts` | JS: 1.20 KB gzip own chunk |
| Cross-browser (Chromium, Firefox, WebKit) | ☒ fail | see Runs | C2-4 only, the same in all three engines |

## Different screens

| Width | Light | Dark | Notes |
|---|---|---|---|
| 320 | ☒ | ☒ | 100% text passes; 200% text fails (C2-4) |
| 360 | ☒ | ☒ | As 320, in Chromium, Firefox and WebKit |
| 390 | ☒ | ☒ | As 320 |
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
| In context (charts article fixture) | ☑ | ☑ | Added as `regionslope` |

## Issues found

| # | Area / screen | Problem | Expected, fix and test |
|---|---|---|---|
| C2-4 | Zoom: 200% text / 320, 360, 390, light and dark; Chromium, Firefox, WebKit | The plot's gutters are fixed at 4.5em (left values) + 10em (right values and names) = 14.5em, 391px at 200% text, wider than the column on any screen under 408px: the page scrolls sideways (408px in a 320–390 viewport) and the lines' area has negative width | No sideways scroll at 200% text (accessibility.md, WCAG 1.4.4 / 1.4.10). The fix is not small: the right gutter must shrink on narrow plots, and the server-side label layout (18 characters a line, `SLOPE.lineChars`) then wraps names into more lines than it plans for, so labels would overlap. Options: a narrow layout (names above the right values, or the dumbbell of the same items) chosen by a container query, as the Sankey does. Failing test: `charts-2.tester.spec.ts` "Slope/default: reflows at 200% text size and 200% zoom with no overlapping labels" and the same for "Slope/edge-cases". **Open: sent back to the builder** |
| C2-2 | Interaction / 320 | A centred tooltip (long item names) ran past the screen's left edge | **Fixed** in the shared `ChartTooltip` |

## Accepted exceptions and open questions

| Exception or question | Reason | Approved by |
|---|---|---|
| O-4: in a crowded slope with nothing highlighted (9 items), labels nudged apart can sit a line or two from their dot, and the labels are in ink, so which close line is which depends on order | No overlap (tested); the table holds every value. A short leader or a key dot beside each right label would tie them; a design call | Question for Karthik |
| Arrow keys step through items in drawing order (the highlight last), not top to bottom | Every item is reachable; the tooltip names it | Noted |
| Light test scope; no screenshot baselines; screen readers not checked by hand | As in the Waterfall sign-off | Karthik (standing); shared |

## Test files

- `site/tests/unit/charts-2.tester.test.ts`, `site/tests/component/charts-2-islands.tester.test.tsx`, `site/tests/e2e/components/charts-2.tester.spec.ts` (tester)
- `site/tests/unit/charts-2.test.ts`, `site/tests/e2e/components/charts-2.spec.ts` (builder)

## Runs

Shared with the other Charts II components; counts in the Sankey sign-off. Slope failures: C2-4 only, 6 runs in Chromium
(320, 360, 390 × default and edge cases) and 4 in Firefox and WebKit at 360.

## Sign-off

☐ Not signed off: C2-4 open (sent back to the builder). Re-test the Zoom area at 320–430 in all three engines after the fix.
Signed (fail): Opus 5.5 component tester, 21 November 2026
