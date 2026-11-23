# SSR page templates — test sign-off

Status: **signed off** (pending Karthik's approval of the fixture home page, which #14 asks for separately)

Scope: the page templates of #14, merged in #59 (SSR core), #57 (static pages) and #64 (search, feeds, preview, report):
`src/views/Shell.astro`, `src/views/Views.tsx`, `src/views/views.css` over `src/server/views.ts`, and the static pages
(`src/layouts/StaticPage.astro`, `src/pages/*.astro`). Home (fixture curations `normal`, `withdrawn-lead`, `region`,
`empty-latest`), region view, Latest and its cursor page, category and sub-category hubs (20 per page, region filter,
an empty filtered hub), entity hub, `/search` (form, results, empty, too many words, daily limit), the 404, the ten
static pages and print.

Tester: Opus 5.5 component tester (not the builder) · Builders: Sonnet 5.5 feature builder and Opus reviewers (#14) ·
Date: 23 November 2026
Branch / commit: `phase-6/site/ssr-templates-test` from `main` at 7fc13be; tests, one fix and this sign-off in the
commits after it.

## Tests

| File | What |
|---|---|
| `tests/component/ssr-templates.tester.test.tsx` | Every template and fixture set rendered to the Worker's markup (`Views` over `loadView`): axe WCAG 2.2 AA in light and dark, heading outline (one h1, no skipped level), unique landmark names, every link a site path or a source's https address, no `undefined`/`NaN` in text; the states (empty Latest, withdrawn lead, Latest 20 + rest, empty filtered hub, daily limit, empty search with and without filters) |
| `tests/e2e/ssr-templates.tester.spec.ts` | The built Worker in workerd (`wrangler dev`), one server per fixture set (`tests/e2e/ssr-servers.ts`, `playwright.config.ts`): per template and screen, no sideways scroll, no text under 12px, targets ≥ 24px, the two typefaces only, nothing off screen or overlapping, axe in both themes (system and `data-theme`), all content kept at 200% text, reflow at 200% zoom; print, forced colours, motion; layout per breakpoint (lead block 1.7fr/1fr from 820px, section rows ≥ 220px); behaviour (back-fill, empty Latest, region view and its Older page, region filter, Latest at 20, sub-categories, entity strip and years, 404 status and robots); no JavaScript on the listing pages; the performance budget |
| Builder's, re-run | `src/server/*.test.ts`, `src/home/compose.test.ts`, `src/static/*`, `tests/astro`, `tests/unit` (875 passed); `tests/ssr/home-render.test.ts` (render budget) |

## All areas

| Area | Result | Tests | Notes |
|---|---|---|---|
| Rendering | ☑ pass | jsdom: every template × fixture set; e2e "what each shows" | Home (lead, side, Latest, sections), region view, hubs, entity hub, search states, 404, static pages |
| Data | ☑ pass | `src/server/views.test.ts`, `src/home/compose.test.ts` (builder, re-run); jsdom "the states"; e2e back-fill, empty Latest, Latest 20 then 9 | Slot text is the page record's; the withdrawn page appears nowhere; a slot whose data is missing keeps its story; figures carry their source (cited figure and popover) |
| Content edge cases | ☑ pass | e2e "nothing off screen or overlapping" on every width; jsdom empty states | Empty Latest (no strip, no message), empty filtered hub ("Show all regions"), empty search, too many words, empty site |
| Formatting | ☑ pass | builder's formatter and listing tests; jsdom and e2e read the date line and hub dates | Dates from the record (`11 Jan 2025`, `Wednesday 15 January 2025`); figures through the shared formatters |
| Server rendering | ☑ pass | e2e "complete static HTML, with no script at all" (JavaScript off); `tests/integration/ssr-build.test.ts` (builder) | Home, hub, entity hub, search: complete without JavaScript, 0 scripts |
| Design-system compliance | ☑ pass | e2e two typefaces only, 12px floor, targets ≥ 24px, lead 1.7fr/1fr from 820px and stacked below, section rows ≥ 220px; token lint (builder) | `views.css` tokens only; no boxes, radius or shadow on content; crimson only on the wordmark and current-item underlines |
| Accessibility | ☑ pass | jsdom axe light and dark on 19 cases × 2; e2e axe in the system theme and the other by `data-theme` on every screen run, three engines; heading outline; unique landmark names | Zero violations of any impact. Screen readers: none by hand (no NVDA, VoiceOver or TalkBack here); roles, names and outline asserted instead |
| Interaction | ☑ pass | e2e region filter to region view, sub-category link, Older links; builder's `ssr-rest.spec.ts` (search form, report form) | Links only on listing pages; no JavaScript |
| Themes (light, dark ×2, forced colours, print) | ☑ pass | e2e every template: system theme plus `data-theme` flip; `chromium-forced-colors-*`; `chromium-print` | Print hides every nav, the region filter, Older, search and footer links, keeps every headline, on a transparent (white) page |
| Motion | ☑ pass | `chromium-motion`; reduced motion in every other project | Nothing animates on load; transitions ≤ 200 ms |
| Zoom (200% zoom, 200% text) | ☑ pass | e2e every template on every screen run | All text kept, no sideways scroll, nothing off screen; 200% zoom from 640px (the 320 run is the 400% case) |
| Failures (images, video, data) | ☑ pass | jsdom daily-limit page; `src/server/serve.test.ts` (503, `Retry-After`, stale copy); `views.test.ts` missing data | No images or video on these templates. D1 failing cannot be forced in wrangler's local D1, so the 503 page is checked in jsdom and the Worker path in Vitest |
| Performance (JS budget, CLS, render time) | ☑ pass | e2e performance budget (Chromium, 360 and 1440); `tests/ssr/home-render.test.ts` | JS: 0 KB on home, hubs, search and 404 (`/report` keeps its form island). See Performance |
| Cross-browser (Chromium, Firefox, WebKit) | ☑ pass | Chromium every width; Firefox and WebKit at 360 and 1440 | See Runs |

## Different screens

| Width | Light | Dark | Notes |
|---|---|---|---|
| 320 | ☑ | ☑ | Run in light; dark by `data-theme` in the same test (axe, contrast, theme tokens) |
| 360 | ☑ | ☑ | Chromium and Firefox light, WebKit dark |
| 390 | ☑ | ☑ | Run in dark, light by `data-theme` |
| 430 | ☑ | ☑ | Run in light |
| 600 | ☑ | ☑ | Run in dark |
| 768 | ☑ | ☑ | Run in light |
| 980 | ☑ | ☑ | Run in dark |
| 1120 | ☑ | ☑ | Run in light |
| 1440 | ☑ | ☑ | Chromium and Firefox dark, WebKit light |
| 1920 | ☑ | ☑ | Run in light |
| Landscape phone 844×390 | ☑ | ☑ | Run in dark |
| Touch (phone, tablet) | ☑ | ☑ | Touch emulated under 980px; targets ≥ 24px; nothing on these pages needs hover |
| Scale factors 1× / 2× / 3× | ☑ | ☑ | 3× under 600px, 2× to 1120px, 1× above |
| In context | ☑ | ☑ | The templates are the context: the list components inside home and hubs, the shell around every page |

Runs:

Light scope (Karthik's standing rule for this laptop): every width in Chromium, Firefox and WebKit at 360 and 1440
only; Playwright with 1 worker on `E2E_PORT=4337` against one `NEWS_FIXTURES=1 NEWS_PREVIEW=1 CLOUDFLARE_ENV=local`
build; Vitest with `--maxWorkers=1`. Each width ran one theme by the system setting; the screen test switches to the
other theme by `data-theme` and runs axe again, so both themes are checked at every width. Behaviour that does not
change with the screen (back-fill, links, pagination, the 404, no-JavaScript, the performance budget) runs at 360 and
1440 only.

- **Chromium** (`chromium-light-320, -360, -430, -768, -1120, -1920`, `chromium-dark-390, -600, -980, -1440,
  -landscape`, `chromium-print`, `chromium-motion`, `chromium-forced-colors-light`, `-dark`): 540 runs, 423 passed,
  117 skipped (by design: behaviour tests away from 360 and 1440), **0 failed**, 45 min.
- **Firefox and WebKit** (`firefox-light-360`, `firefox-dark-1440`, `webkit-dark-360`, `webkit-light-1440`; the screen
  test includes the 200% text run): 160 runs, 151 passed, 8 skipped (the performance test is Chromium-only), 1 failed:
  WebKit 1440's no-JavaScript test timed out loading the fourth of four pages in one test (60 s) while the same test
  passed in WebKit 360, Firefox and Chromium. The test was split into one test per page and run once more on
  `webkit-light-1440`: 4 of 4 passed (31 s). Load, not a defect.
- **Builder's smoke specs** (`ssr-rest.spec.ts`, `static-pages.spec.ts`, `shell.spec.ts` on `smoke-light-360`,
  `smoke-dark-1440`): 78 passed, 0 failed.
- **Vitest** (`--maxWorkers=1`): the builder's `src/server`, `src/home`, `src/static`, `src/preview.test.ts`,
  `tests/astro`, `tests/unit`: 875 passed (29 files). Tester's `tests/component/ssr-templates.tester.test.tsx`: 45
  passed; with `src/server/search-view.test.ts` after the SSR-1 fix: 52 passed.
- Not run: Firefox and WebKit at the other widths, landscape, print and motion (light scope); forced colours exist only
  in Chromium.

## Performance

Measured once, on the shared laptop with other agents running; not chased.

| Check | Budget | Result |
|---|---|---|
| Home `/`, first contentful paint / largest contentful paint (Chromium 360 light, 1440 dark) | < 1.8 s / < 2.5 s | 544 / 544 ms; 384 / 384 ms |
| Hub `/economy`, FCP / LCP | < 1.8 s / < 2.5 s | 356 / 356 ms; 300 / 300 ms |
| Layout shift once the fonts are in | 0 | 0 on both pages, both widths |
| CLS since navigation, font swap included (recorded, not asserted) | — | 0 on both pages, both widths |
| JavaScript on home, hubs, entity hub, search, 404 | 0 KB | 0 scripts (JavaScript off and on) |
| Transfer, first load (HTML, CSS, fonts) | — | home 228 KB, `/economy` 201 KB, mostly the two self-hosted fonts |
| Server render in Bun, data in memory, p95 over 3 runs (`tests/ssr/home-render.test.ts`, run while the build ran) | hub ≤ 4 ms; home ≤ 10 ms (test) | hub 0.76–1.22 ms; home 2.15–3.55 ms; home with its slot figure and mini chart 4.4–32 ms (the 32 ms run was during the build; the test takes the best of three) |

## Issues found

| # | Area / screen | Problem | Expected (spec), fix and test |
|---|---|---|---|
| SSR-1 | Content / all screens, `/search` with a filter and no results | The empty state always linked to "See the latest pages", even when a filter caused the empty result | design/components.md "Empty state" and layout.md "Search and latest": offer to clear the filters. Fixed in `Views.tsx`: with any filter set the action is "Clear the filters" (`/search?q=<query>`); without, Latest as before. Test: `ssr-templates.tester.test.tsx` "an empty search with filters offers to clear them" |

Larger gaps, not fixed here (the template follows the data model or ADR 0010; the design documents need to catch up,
or the data model needs fields):

| # | Where | Gap | Owner |
|---|---|---|---|
| G-1 | Category and entity hub heads (layout.md) | No one-line description; entity head has no region, sector or official website. `TaxonomyNode` and `EntityRecord` have no such fields | Data model decision (#6 contracts) or a layout.md change; docs owner #18 |
| G-2 | Hub pagination (layout.md says "Newer", "Older" and page numbers) | Templates show "Older" only, as ADR 0010 section 4 decides (cursors) | layout.md should follow ADR 0010 (#18) |
| G-3 | Search daily-limit message (components.md: "Browse by category") | The action links to "See the latest pages"; the masthead's category links are on the page | Copy decision; small change in `Views.tsx` if wanted |

## Accepted exceptions and open items

| Exception | Reason | Approved by |
|---|---|---|
| The search daily-limit page (503) is not tested in a browser | wrangler's local D1 cannot be made to fail; its markup is axe-checked in jsdom in both themes, and its status, `Retry-After` and `no-store` in `serve.test.ts` and `search-view.test.ts` | Tester |
| Charts print in light colours (layout.md "Print") is not checked with a dark system theme | Shared open question from earlier sign-offs (print uses the system theme's tokens); the print project runs light | Pending Karthik |
| Screenshot baselines | Compared only in the Playwright Docker image; none exist for any page yet (site/TESTING.md) | Pending, as for every component |
| SSR-1's fix is proven in jsdom, not yet in the browser | The e2e build used here predates the fix; CI rebuilds | Tester |

## What's left

- **Karthik approves the fixture home page** (#14 "Done when"; record the approval in the PR). See it with
  `bun run build` under `CLOUDFLARE_ENV=local`, then `bun run preview` (`/`, and the other curations with
  `--var NEWS_FIXTURE_SET:withdrawn-lead|region|empty-latest`).
- Screenshot baselines in the Docker image (`bun run test:visual`), as for every component: none exist yet.
- The Firefox and WebKit widths not run (see Runs), on a quiet machine.

## Sign-off

☑ Every area and every screen above is passed or has an accepted exception or a recorded open item.
Signed: Opus 5.5 component tester, 23 November 2026
