# LineChart — test sign-off

Status: **signed off after the #8 fix round re-test (20 Nov 2026); see the last section**

Tester: Opus 5.5 component tester (not the builder) · Builder: Opus 5.5 component builder (issue #8) · Date: 18 November 2026
Branch / commit: `phase-3/components/data-charts-1`, components at e1d5d0c; tests, fixes and this sign-off in the commit after it

## All areas

| Area | Result | Tests | Notes |
|---|---|---|---|
| Rendering | ☑ pass | `data-charts.test.ts`; `chart-islands.test.tsx`; `data-charts.tester.test.ts` | Every prop: title and subtitle with references, highlight, given/value sort (bars), several series and legends, markers (step line); fixture pages default and edge cases |
| Data | ☑ pass | `data-charts.test.ts` edge cases; `data-charts.tester.spec.ts` "missing values are hatched" | Every mark, label and tooltip carries its data reference; unknown series → frame with "No data" and no island; null → gap or hatched mark and "No data" in the table; zero, negative, ≥ 1e12 and 1.4e-8 keep their digits; long series thin labels; single point drawn; other-unit series left out, never converted. CSV equals the data table for every fixture  |
| Content edge cases | ☑ pass | `data-charts.tester.spec.ts` "nothing clipped or off screen"; `data-charts.spec.ts` "reads at this screen size" | Long titles, long category labels, many periods, one item, every value missing |
| Formatting | ☑ pass | `data-charts.tester.test.ts`; `data-charts.tester.spec.ts` "rates keep the source's digits" | Rates and points keep the source's digits in labels, tooltips and tables (0.25%, 0.50%, 1.00%, 4.50%); $/€/£/₹/₩/NT$/HK$/A$/yen units written by code; true minus; thousands separators; period labels verbatim |
| Server rendering | ☑ pass | `data-charts.spec.ts` "chart pages are complete without JavaScript and only the plots hydrate"; `chart-islands.test.tsx`; `tests/ssr` | Marks, values and table present without JS; no dead tab stop before hydration; only the plot hydrates; no hydration warnings |
| Design-system compliance | ☒ fail | `chart-colours.test.ts` (validator, both themes); `data-charts.tester.test.ts` and `data-charts.tester.spec.ts` DC-1 | DC-1: a highlighted subject that fell is crimson. Tokens only; one highlight at most; palette validated in both themes; no radius, shadow or animation |
| Accessibility | ☑ pass | `data-charts.spec.ts` axe both themes, "each plot is named by its data summary"; `data-charts.tester.spec.ts` forced colours, "one tab stop" | axe zero violations in light and dark on every screen run; plot named by its summary; data table reachable by keyboard; one tab stop; hit areas ≥ 24px. Screen readers: none by hand |
| Interaction | ☑ pass | `data-charts.spec.ts` keys and tap; `data-charts.tester.spec.ts` hover tooltip, last-point tooltip on screen, one tab stop | Arrows, Home, End move and the tooltip follows; Escape closes; tap opens, tap outside closes; hover snaps (line); tooltips stay on screen at every width |
| Themes (light, dark ×2, forced colours, print) | ☑ pass | `data-charts.spec.ts`; `data-charts.tester.spec.ts` forced colours (marks visible), print | Dark by system and `data-theme`; forced colours axe-clean with every mark drawn; print hides actions and links |
| Motion | ☑ pass | `data-charts.tester.spec.ts` motion tests | Nothing animates on load; hover lightening ≤ 120 ms only when motion is allowed; 0 ms under reduced motion |
| Zoom (200% zoom, 200% text) | ☒ fail | `data-charts.tester.spec.ts` "reflows at 200% text size and 200% zoom" | DC-2 at 320-430 with 200% text |
| Failures (images, video, data) | ☑ pass | `data-charts.tester.spec.ts` "charts still read when their island scripts fail to load"; edge-case fixtures | Island script blocked: static marks, values and table still read, no dead tab stop. Missing data: see Data |
| Performance (JS budget, CLS, render time) | ☑ pass | `tests/integration/budgets.test.ts`; `data-charts.tester.spec.ts` layout shift; `tests/ssr` | JS: the island page within ADR 0001's chart budget (integration test). Shift after fonts < 0.001. Render time: see Runs |
| Cross-browser (Chromium, Firefox, WebKit) | ☒ fail | all specs on the projects under Runs | Failures are DC-1/DC-2 only, the same in every engine run |

## Different screens

| Width | Light | Dark | Notes |
|---|---|---|---|
| 320 | ☐ | ☐ | FAIL DC-2 at 200% text (100% text passes) |
| 360 | ☐ | ☐ | FAIL DC-2 at 200% text (100% text passes) |
| 390 | ☐ | ☐ | FAIL DC-2 at 200% text (100% text passes) |
| 430 | ☐ | ☐ | FAIL DC-2 at 200% text (100% text passes) |
| 600 | ☑ | ☑ | |
| 768 | ☑ | ☑ | |
| 980 | ☑ | ☑ | |
| 1120 | ☑ | ☑ | |
| 1440 | ☑ | ☑ | |
| 1920 | ☑ | ☑ | |
| Landscape phone 844×390 | ☑ | ☑ | |
| Touch (phone, tablet) | ☑ | ☑ | Popover and tooltips open on tap; targets ≥ 24px |
| Scale factors 1× / 2× / 3× | ☑ | ☑ | 3× phones, 2× tablets and small laptops, 1× desktops |
| In context (article, home fixtures) | ☑ | ☑ | KeyFigures, charts and tables on `/fixtures/renderer/charts/`: in order, ≥ 16px apart, axe-clean in both themes. No home fixture page exists yet |

DC-1 fails on every width, theme and engine run (it is a colour rule, so it is listed under Design-system compliance, not against a screen).

Runs:

Each Chromium project ran one theme by the system setting; every axe and colour test also switches to the other theme
by `data-theme` inside the test, so both themes are checked at every width.

- **Chromium** (`chromium-*`): light at 320, 360, 430, 768, 1120 and landscape; dark at 390, 600, 980, 1440, 1920 and
  landscape; print; motion; forced colours light and dark. First slice (light 360, dark 1440): 270 runs, 227 passed, 27 skipped, 16 failed (13 DC-1/DC-2, 3 tester test bugs fixed and re-run green). Matrix (the other 14 projects): 1,422 runs, 1,241 passed, 126 skipped (by design: `@visual` outside Docker, 200% zoom below 640px, hover-only or touch-only tests), 55 failed: DC-1 40, DC-2 11, and 4 from a tester test bug (hatching counted as invisible) fixed and re-run with DC-3: 44 of 44 passed.
- **Firefox**: light 320, dark 1440, print. WebKit and Firefox together: 578 runs, 500 passed, 52 skipped, 26 failed: DC-1 16, DC-2 9, 1 Firefox "Page crashed" under load that passed on re-run.
- **WebKit**: dark 390, light 1120, motion (counts with Firefox above).
- Not run: the remaining Firefox and WebKit widths (the laptop was at 100% CPU from other agents; one engine-width
  project took about 7 minutes in Chromium). Forced colours exist only in Chromium.
- Vitest (unit, component, astro; `--maxWorkers=1`): 838 tests, 833 passed, 5 failed: DC-1 (4: three tester tests and the builder's own crimson test on the new fixtures) and the token-lint test timing out at 5 s under load (the lint itself passes).
- SSR in Bun (`bun test tests/ssr`): 73 passed, 0 failed, re-run at the end: island page render p95 10.1 ms (budget 50), primitives p95 at most 4.75 ms (budget 20). No load exception needed.
- Integration (HTML validity, links, budgets, `--maxWorkers=1`): 173 passed (4 files), with the tester's fixtures included.

Screenshot baselines: none made or accepted. `@visual` tests compare only inside the Playwright Docker image
(`bun run test:visual`, site/TESTING.md); no Linux baselines exist for these components yet, so there is no baseline to
justify. To do before merge with Docker available.

Screen readers: not checked by hand (no NVDA, VoiceOver or TalkBack on this machine). Roles, names and the accessibility
tree are asserted instead, and axe runs in both themes on every screen run.

## Issues found

| # | Area / screen | Problem | Expected (spec), fix or failing test |
|---|---|---|---|
| DC-1 | Design-system compliance / every screen, light and dark, Chromium (Firefox and WebKit where run) | A highlighted subject that fell is drawn in crimson: the highlighted negative bar (Greater China, −8.0%) and its bold label; the highlighted column for a period that fell (−2.6% in 2023, after −0.4%); a highlighted line that ended below where it started (US CPI 3.1% → 2.9%). `colourRoles` and `barPlot` give the subject the `highlight` role whatever its value. The existing `highlightContext` LineChart edge case (sr_fx_gaps, 1.5 → 0.9) is crimson too | Karthik, 16 Nov 2026: crimson never marks a subject that fell; a highlighted subject that fell uses the neutral highlight. No neutral-highlight token exists: add one (or use an ink token) via the token procedure, decide what "fell" means per chart type (bar value < 0; column below the previous period; line last < first) and write it into charts.md and color.md (W3 docs owner, Karthik's approval). Failing tests: `tests/unit/data-charts.tester.test.ts` "crimson never marks a highlighted subject that fell (DC-1)" (3), `tests/e2e/components/data-charts.tester.spec.ts` same describe (3 per screen), and the builder's own `data-charts.test.ts` "no fixture shows crimson … no fall is crimson" and `data-charts.spec.ts` "no fall is shown in crimson", which now fail on the new `fell` fixtures. **Open: sent back to the builder** |
| DC-2 | Zoom (200% text size) / 320, 360, 390, 430, light and dark; Chromium, Firefox (320) and WebKit (390) alike | At 200% text on a phone, labels placed by estimated width at 100% text leave the plot: the last x label of a column chart ("fiscal 2024", page 347px wide at 320, 409px at 390); a line chart's end label for a long value ("0.000000014", page 383px at 320) while the y-axis labels squeeze the plot to nothing (x labels "2019/2020" overprint); a step-line event label ("Rate cut, 18 Sep 2024") pushed off the left edge (−69px at 320) and the edge-case step line making the page 397px wide. 600px and wider pass | accessibility.md and WCAG 1.4.4: 200% text, no lost content, no sideways scroll. Measure labels in the browser (or in `em`) rather than estimating at the default size, wrap or drop labels that do not fit, and keep a minimum plot width. Failing test: `data-charts.tester.spec.ts` "<page>: reflows at 200% text size and 200% zoom" for ColumnChart/default, ColumnChart/edge-cases, LineChart/edge-cases, StepLineChart/default, StepLineChart/edge-cases. **Open: sent back to the builder** |

## Accepted exceptions and open questions

| Exception | Reason | Approved by |
|---|---|---|
| Font-swap layout shift since navigation (recorded as an annotation, not asserted) | Shared open design question (fallback faces with `size-adjust`), as for #4; the shift after the fonts load is asserted < 0.001 and passes | Pending Karthik |
| Print uses dark tokens when the system is dark (layout.md: charts print in light colours) | Shared open design question from #4 | Pending Karthik |
| `token-lint.test.ts` timed out (5 s) under load in the Vitest run | `bun scripts/lint-tokens.ts` passes on the whole tree; the timeout is the shared CPU, not a token violation | Tester (load only) |

## Test files

- `site/tests/unit/data-charts.tester.test.ts` (tester): rate digits, units, true minus, KeyFigures 2-4, colour roles, DC-1
- `site/tests/e2e/components/data-charts.tester.spec.ts` (tester): every fixture page on every screen (clipping, 200% text and zoom, motion, CLS, forced colours, print), island scripts blocked, popovers click-only, one tab stop, hover and tap tooltips, sticky first column, rate digits, missing-data hatching, DC-1, in context; `galleryChecks` for ChartFigure, DataLinks, SourcePopover and Sparkline
- `site/tests/unit/data-charts.test.ts`, `site/tests/component/chart-islands.test.tsx`, `site/tests/unit/chart-colours.test.ts`, `site/tests/e2e/components/data-charts.spec.ts` (builder): references, edge cases, CSV = table, keys and taps, colour validation, budgets
- `site/tests/ssr/`, `site/tests/integration/budgets.test.ts`: server rendering, render time, JS budgets
- Fixtures added by the tester: `fell` in BarChart, ColumnChart and LineChart edge cases; `rates` in KeyFigures edge cases

## Sign-off

☐ Not signed off: DC-1 and DC-2 open (sent back to the builder). Re-test the affected areas and screens after the fix.
Signed (fail): Opus 5.5 component tester, 18 November 2026

## Re-test: #8 fix round (20 November 2026)

Status after re-test: **pass, signed off with the open exceptions below**

Tester: Opus 5.5 component tester (not the builder) · Fix round at e603b79; re-test fixes, tests and this section in the commits after it on `phase-3/components/data-charts-1`.
Scope: only what the fix round changed (DC-1, DC-2, the citation tests). The other areas stand as signed above.

DC-1: line, end marker, legend key and bold end name take the neutral highlight when the last shown value is below the first (gaps at both ends skipped); a single point, an equal first and last value, a flat line and all-negative-but-rising lines stay crimson; a comparison stays blue. DC-2: tight layout verified; fixed R1-2 and R1-3 (shared plot code).

| Item | Result | Tests |
|---|---|---|
| DC-1 rules and edge cases | ☑ pass | `tests/unit/data-charts-r1.tester.test.ts` (57) |
| DC-1 colours in the browser, light and dark (system and `data-theme`), forced colours | ☑ pass after R1-1 | `tests/e2e/components/data-charts-r1.tester.spec.ts` "DC-1" |
| `chart.neutral-highlight` contrast | ☑ pass | 9.89 / 8.95 on paper / wash (light), 8.16 / 7.14 (dark); `chart-colours.test.ts` |
| Distinct from comparison and context (dataviz validator, `--pairs all`) | ☑ pass | CVD worst ΔE 16.7 light, 14.3 dark; normal vision 17.4 / 17.2; vs `chart.neutral` hatch ≥ 33 |
| DC-2 at 100% and 200% text, 320–1440 | ☑ pass after R1-2…R1-4 | same spec, "DC-2" |
| At 100% text nothing changes | ☑ pass | label audit identical on the pre-fix build (6018247) and after, at 320, 360, 430, 768, 1440 |
| Whole Vitest suite at e603b79 (citation tests) | ☑ pass | 848 Vitest + 73 Bun SSR, 0 failed |

The validator FAILs lightness band and chroma floor for the neutral highlight (and for context). Those are categorical-palette checks; the neutral highlight is a single achromatic role, always directly labelled and backed by the table, so they do not apply. Every pair check passes. It is never on a chart with crimson (one highlight), so the crimson pair (CVD ΔE 7.0 dark) does not arise.

Issues found and fixed in this round (small, inside the fix's scope; pushed as normal commits):

| # | Area / screen | Problem | Fix |
|---|---|---|---|
| R1-1 | Themes: forced colours, Chromium light and dark | A fallen highlighted table row kept its neutral bar instead of `Highlight` (the `.is-fell` rule outranked the forced-colours rule) | DataTable.css forced-colours selector covers `.is-fell`. Test: "forced colours: the fallen subject is drawn in Highlight" |
| R1-2 | Zoom: 320 and 360, 200% text | A tight x label that wraps to three lines ("26 January 2022") ran into the end-label row: the axis reserved two lines | Tight x axis is one grid cell its labels share, as tall as its tallest label |
| R1-3 | Zoom: 320 and 360, 200% text | The phone event label on a tight step line wrapped to four lines and ran over the title above the plot | Tight plots show it in a row above the plot that grows with it |
| R1-4 | Zoom: 320–430, 200% text | Tight column charts kept every value label: values overlapped and the last ran past the plot and above it | Tight column charts show only the highlighted and latest values, edge-aligned, with room above the tallest column |

This component: R1-2, R1-3.

Open exceptions (listed in `tests/e2e/components/data-charts-r1.known.json`; all measured identical on the pre-fix build except the last):
- 768 and 1440 at 200% text: plots are 20em or wider, so not tight; x labels chosen for 100% text overlap and sit below the axis. Not part of DC-2 (phones); needs its own fix.
- 100% text, unchanged by design: last x label or end label a few px past the plot edge at 320–430; a negative column's value over the first x label at 768/1440; x labels of the 1e17 and 1e-8 edge fixtures touching at 320.
- 320 at 200% text: the 1e-8 edge fixture's latest value (eleven digits) is wider than the 114px left for the columns and crosses an axis tick. The figure stays in the tooltip and the table.
- Rule question for Karthik: bars and columns "fell" only below zero, so a column chart of levels whose highlighted period is lower than the one before stays crimson. Sparkline's latest-point dot is crimson whatever the direction (outside this fix).
- Screen readers not checked by hand; no visual baselines (Linux Docker only).

Runs (laptop under load, 2 workers):
- Chromium re-test spec: DC-2 at 320, 360, 430, 768, 1440 × light, dark: 120 passed; DC-1 at light 320/430, dark 360/1440, forced colours light and dark: 30 passed.
- Regression, existing builder and tester specs for these charts: Chromium light 360 + dark 1440: 118 passed, 12 skipped (by design). Firefox and WebKit light 360 + dark 1440, with the re-test spec (includes 200% text): Firefox 158 passed, 10 skipped; WebKit 156 passed, 12 skipped.
- One trace-file write error at 768 (ColumnChart edge cases, 100%) in an early run: load-flaky; passed in the full run.
- Vitest: fall-rule tests (re-test file, model, chart colours) after the fixes: 90 passed.

Signed (pass, with the exceptions above): Opus 5.5 component tester, 20 November 2026
