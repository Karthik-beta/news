# ChoroplethMap and chart social images — test sign-off

Status: **signed off** (23 November 2026), with the exceptions and open items below

Tester: Opus 5.5 component tester (not the builder) · Builder: Opus 5.5 component builder (issue #13, PR #63) · Date: 23 November 2026
Branch / commit: `phase-3/charts/choropleth`, built at 987ad08 (renderer 1.1.0 merge 28823b7); tests and fixes in the tester's commit on top.
Scope: LIGHT (Karthik's standing rule): every screen in Chromium; Firefox and WebKit for the choropleth specs at 360 and 1440 plus one 200%-text run; Vitest with `--maxWorkers=1`; Playwright with 1 worker on `E2E_PORT=4335`.

Fixtures: `/fixtures/components/ChoroplethMap/default/` (`jobless`: IMF unemployment, 108 countries; `g7`), `/fixtures/components/ChoroplethMap/edge-cases/` (`gaps`, `one`, `same`, `sizes`, `tiny`, `allnull`, `unknown`); in context, the tester added `g7map` to the charts article fixture `/fixtures/renderer/charts/` (seven IMF points, their period and source copied into that bundle).

## All areas

| Area | Result | Tests | Notes |
|---|---|---|---|
| Rendering | ☑ pass | `tests/unit/choropleth.test.ts`, `tests/unit/choropleth.tester.test.ts` | Legend (1–5 classes, "No data" key), map, ranked table, ranked bars; one class labelled with its value |
| Data | ☑ pass | `choropleth.tester.test.ts`, `choropleth.test.ts`, `data-charts.test.ts` | Shades rise with values and equal values share a shade on every fixture; every legend class holds a country (fixed, issue 1; 2,000 random value sets); table rows, bars and shaded regions are one list; every value cited (`data-ref`); null, unknown ref, other unit, code with no shape, zero, ≥ 1e12, 0.0004 |
| Content edge cases | ☑ pass | fixtures; tester specs | One country, all equal, long name (wraps in bars and table), specks (Luxembourg, Malta), all null and all unknown (frame says No data, no map) |
| Formatting | ☑ pass | `choropleth.tester.test.ts` | Breaks in the display unit (trn: "0.02", "1"), thousands separators ("10,000", "100,000%"), small values keep digits ("0.0004"), % only on the top break; class words "under 3", "3 to 5", "8% or more" |
| Server rendering | ☑ pass | `choropleth.tester.test.ts`; `choropleth.tester.spec.ts` (scripts blocked) | The map is static SVG; the only island in a map figure is the phone's `BarChartPlot`; each map's hatch pattern id is unique on the page; with JavaScript off the map, legend, table and bars are all there |
| Design-system compliance | ☑ pass | `choropleth.tester.spec.ts`; `bun run lint:tokens` | Every region and swatch is the token for its step in both themes, by system setting and `data-theme` either way (light 300–700; dark 600, 400, 300, 200, 100, reversed so more reads as more contrast on dark paper); borders `--color-paper`; hatch and no-data outline `--chart-neutral`; never crimson; nothing highlighted |
| Accessibility | ☑ pass | `choropleth.spec.ts`, `choropleth.tester.spec.ts`, `choropleth.tester.test.ts` | axe zero violations light, dark and forced colours. The map is `img` named by the summary ("… 108 countries …"); the ranked table and bars are now "Ranked: <title>" (fixed, issues 2 and 3), so the summary is read once and no two tables share a name with "Show the data table" open. Legend is an `ol` of class words, edge labels `aria-hidden`. Screen readers by hand: none (none on this machine) |
| Interaction | ☑ pass | `choropleth.tester.spec.ts` | The map takes no tab stop; on phones one stop on the bars, Home/arrows step through them with the tooltip; no tooltips on the map (by design, charts.md) |
| Themes (light, dark ×2, forced colours, print) | ☑ pass | `choropleth.tester.spec.ts` | Forced colours keep one fill per step, the hatch and borders (`forced-color-adjust: none`); print keeps shades and hatch (`print-color-adjust: exact`) |
| Motion | ☑ pass | `choropleth.tester.spec.ts` | Nothing animates; no transitions on the map (bars' own ≤ 200 ms); no layout shift after fonts |
| Zoom (200% zoom, 200% text) | ☑ pass | `choropleth.tester.spec.ts` | Legend labels inside the figure and apart, no sideways scroll (fixed, issue 4); the narrow/wide view switches at exactly 36em of the map's own text at 100% and 200% |
| Failures (images, video, data) | ☑ pass | `choropleth.tester.spec.ts`; edge-case fixtures | Scripts blocked; all-null and unknown data say No data |
| Performance (JS budget, CLS, render time) | ☑ pass | `tests/integration/budgets.test.ts` | No new island (bars reuse `BarChartPlot`); geometry 36 KB / 12 KB gzip within its 40/15 KB budget; CLS after fonts 0 |
| Cross-browser (Chromium, Firefox, WebKit) | ☑ pass | see Runs | Chromium every screen; Firefox and WebKit at 360 and 1440 in both themes with 200% text and zoom (LIGHT scope). One Playwright-Firefox tooling crash, worked around (Runs) |

## Different screens

Chromium ran every project below for `choropleth.spec.ts`, `choropleth.tester.spec.ts`, `social-image.spec.ts` and `social-image.tester.spec.ts`. Firefox and WebKit: 360 and 1440 (light and dark), choropleth specs, 200% text and zoom included.

| Width | Light | Dark | Notes |
|---|---|---|---|
| 320 | ☑ | ☑ | Small map and ranked bars; 200% text pushed the page to 497px before fix 4 |
| 360 | ☑ | ☑ | Same; hatch 0.45px lines at 3× = 1.35 device px |
| 390 | ☑ | ☑ | Bars (19–22em): the builder's smoke test assumed a table above 360px; corrected to the 36em rule |
| 430 | ☑ | ☑ | Bars |
| 600 | ☑ | ☑ | 36.8em: full map and table at 100% text; bars at 200% text (18.4em) |
| 768 | ☑ | ☑ | Table; bars at 200% text (24em) |
| 980 | ☑ | ☑ | Table; bars at 200% text |
| 1120 | ☑ | ☑ | |
| 1440 | ☑ | ☑ | Table; map 720px, hatch 1.08px at 1× |
| 1920 | ☑ | ☑ | |
| Landscape phone 844×390 | ☑ | ☑ | 48em: table; bars at 200% text |
| Touch (phone, tablet) | ☑ | ☑ | Bars take taps (BarChartPlot's own tests); map has no targets |
| Scale factors 1× / 2× / 3× | ☑ | ☑ | Hatch at least one device pixel wide at every map size (test) |
| In context (article, home fixtures) | ☑ | ☑ | Charts article fixture (`g7map`): same gap above it as above the heatmap; 200% text. Not on the home fixture (no chart there) |

## Social images

`scripts/social-image.ts`, Chromium, 1200×675, light: line chart, heatmap, small multiples (both default fixtures), bar, histogram, stacked bar, dot plot and maps make the card with nothing under 20px (smallest 20.8px) and everything inside the padding (`social-image.tester.spec.ts`). These fail, by design, with an error that names the chart and its height against the card's room (301 CSS px): Sankey (both fixtures, 519px; the layout is at least 26rem tall), Waterfall `profit` (426px), BarChart `long` (592px), Slope `crowded` (417px), Heatmap `matrix` (376px); SmallMultiples `many` was 518px before fix 5 and was not re-measured after it. Sankey, Waterfall, no data and a missing id are asserted in `social-image.tester.spec.ts`. A chart with no data now fails with "has no data to show" (fixed, issue 6); a missing chart id says "no chart … on the page".

## Issues found

| # | Area / screen | Problem | Fix |
|---|---|---|---|
| 1 | Data, all screens (`gaps`) | A legend class could be empty: quintile breaks 3, 9, 10 left "9 to 10" with no country, because 8.75 rounded to a break of 9 with 12.5 above; the code comment claimed no class was ever empty | `mapClasses` keeps a break only when a value lies between the last break and it (`classes.ts`; `design/charts.md`); test over the fixture and 2,000 random sets |
| 2 | Accessibility, phones (< 36em) | The map and the ranked bars were two images with the same long summary, read twice | Bars named "Ranked: <title>"; the map keeps the summary |
| 3 | Accessibility, ≥ 36em | With "Show the data table" open, the visible ranked table and the data table were two tables of the same name | The visible table is named "Ranked: <title>" (hidden "Ranked:" span + title) |
| 4 | Zoom, 320/360 at 200% text | The legend's five 3.5em classes did not shrink: page 497px wide in a 320px viewport (legend 192px past the figure) | `.choropleth__scale { min-width: 0 }` and `.choropleth__class { min-width: 0 }` so the scale shrinks with the figure |
| 5 | Social image, small multiples | Two panels side by side shared the overflow per panel, not per row, so SmallMultiples `margins` and `columns` missed the card by 7px | The overflow is shared between rows of plots |
| 6 | Social image, no data | A chart with no data made an empty card | Error: `chart "<id>" has no data to show` |
| 7 | Test, 390 and 430 | The builder's smoke test expected the table above 360px; at 390/430 the map is under 36em, so bars show (correct per spec) | Test now decides by the map's own em width |

## Accepted exceptions

| Exception | Reason | Approved by |
|---|---|---|
| In a social image an x label centred under the last column may hang into the 48px padding (SmallMultiples `columns`: "fiscal 2024" by 5 image px) | The same overhang the site allows into the page gutter; the label is whole and 43px from the edge. The test holds x labels to half the padding | Tester (minor, visual) |
| At 1× on a phone-sized map the hatch is a fine tint rather than distinct lines (0.43px lines, 1.7px apart, at 320 wide) | Phones are 2–3×, where the lines are 1.3 device px; the grey outline and the legend's "No data" key also mark it | Tester |

## Open for Karthik

- **Charts that cannot make a social image.** Every Sankey (min 26rem plot) and any bar/waterfall/slope with many rows is taller than the card's 301 CSS px and fails, as designed ("an error, never shrunk"). Options: accept (share those charts by screenshot), or let the card grow to a 1200×1200 square for tall charts. Not changed here. **Decided in #71:** tall charts get the 1200×1200 square (design/charts.md, "Tall charts get a square"), tested in `social-image.tester.spec.ts`.
- **Map details proposal** (`design/README.md`) still awaits approval, unchanged by testing.

## Runs

Tester files: `site/tests/unit/choropleth.tester.test.ts` (24 cases), `site/tests/e2e/components/choropleth.tester.spec.ts`, `site/tests/e2e/components/social-image.tester.spec.ts`; the builder's `choropleth.spec.ts` smoke test now decides narrow and wide by the map's own em width.

- `bunx vitest run --project unit --project component --project astro --maxWorkers=1`: 53 files, 2,015 tests pass.
- `bun test tests/ssr`: 173 pass on an idle laptop. Under load (straight after Vitest) three render-time budgets failed (hub 4 ms, primitives 20 ms, island page 50 ms); they are load-flaky: on the idle run the hub p95 was 0.9–1.2 ms and the island page p95 24.5 ms. The island page (apple-fy2024) and the primitives were not touched by this PR.
- `vitest run --project integration` on the `NEWS_FIXTURES=1` build: 5 files, 593 tests pass (HTML validity, links, tokens, JS budgets).
- `bun run typecheck` 0 errors; `bun run lint:tokens` clean; biome clean on every file this PR touches (the repo's other biome findings are in files outside it).
- Chromium, all 26 projects (10 widths × 2 themes, landscape ×2, print, motion, forced colours ×2), the four map and social-image specs, 1 worker: 690 runs, 414 passed, 272 skipped as designed (social images once, 200% zoom below 640px, CLS outside Chromium), 4 failed: the builder's smoke test at 390 and 430 (issue 7); after the fix it passed 8/8 at 390, 430, 600 and landscape.
- Firefox and WebKit, 360 and 1440, light and dark, both choropleth specs (200% text and zoom included): 152 runs, 140 passed, 8 skipped, 4 failed, all one Firefox tooling crash (below). After the change: Firefox passes axe with the data table open and skips only the snapshot step; WebKit passes.

Tooling exception: Playwright's Firefox crashes the page ("Page crashed") when axe and `locator.ariaSnapshot()` both run on this page with the "Show the data table" `<details>` open, in either order; each alone passes. Reproduced outside the test runner with a bare script; not a page defect. The accessible-name checks run in Chromium (every width) and WebKit; Firefox runs axe only for that test.

## Sign-off

☑ Every area and every screen above is passed or has an accepted exception.
Signed: Opus 5.5 component tester, 23 November 2026

## Screenshot baselines (#71)

Accepted 28 Nov 2026 by an Opus component tester that did not build this component. First baselines for the `ChoroplethMap/default` and `ChoroplethMap/edge-cases` fixture pages (`<main>` after every visible island has hydrated): `site/tests/e2e/components/charts-visual.spec.ts`, PNGs in `site/tests/e2e/__screenshots__/chromium-<screen>/components/charts-visual.spec.ts/`. Made in `mcr.microsoft.com/playwright:v1.65.1-noble` with `bun run test:visual:update`; Chromium, light and dark at 360 and 1440, light at 768 and 980 (site/TESTING.md, "Screenshots", says why not the whole matrix). Reason for accepting: no baseline existed; the images were reviewed against design/charts.md (one highlight, no crimson on a fall, labels whole, nothing past the column) and a second run in the image matched every one pixel for pixel. CI compares the 360 and 1440 ones on every PR (`Site screenshots (smoke, Playwright image)`), nightly all of them. This replaces the "no screenshot baselines" exception above for these screens; Firefox, WebKit and the other widths stay without pixel baselines.
