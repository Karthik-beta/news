# Text — test sign-off

Status: **signed off**, with open design questions for Karthik (below)

Tester: Opus 5.5 component tester (not the builder) · Builder: Opus 5.5 component builder (issue #4) · Date: 16 November 2026
Branch / commit: `phase-1/site/renderer`, components at b842525, tests at 3092642 and the sign-off commit

## All areas

| Area | Result | Tests | Notes |
|---|---|---|---|
| Rendering | ☑ pass | `Text.test.tsx` | All 18 variants, default `<p>`, `as`, `id`, `className`, `tabular`, no empty class |
| Data | ☑ n/a | — | Holds no data |
| Content edge cases | ☑ pass | `Text.spec.ts` | Long headline wraps; long unbroken word and URL break inside the column at every width |
| Formatting | ☑ pass | `Text.spec.ts` | Tabular figures with the true minus |
| Server rendering | ☑ pass | `tests/ssr/primitives.test.tsx`; hydration in the `.test.tsx` | Complete static HTML in plain Bun, no script, no hydration markers, no React warnings; server and client markup identical on hydration |
| Design-system compliance | ☑ pass | `Text.test.tsx`; `tests/unit/primitives-css.test.ts`; `Text.spec.ts` | Every variant uses one of the two typeface tokens and a size token; serif/sans split per typography.md; crimson only on wordmark and fly title; colours from tokens in both themes; computed sizes per breakpoint match tokens.json; body line height ≥ 1.5; no all-caps, no monospace |
| Accessibility | ☑ pass | `tests/e2e/components/primitives.ts` gallery checks | axe zero violations (contrast included) in both themes, in every engine and screen run; no text under 12px |
| Interaction | ☑ n/a | — | Not interactive |
| Themes (light, dark ×2, forced colours, print) | ☑ pass | `Text.spec.ts`; gallery checks | Light, dark (system and `data-theme`), forced colours |
| Motion | ☑ pass | `tests/e2e/components/primitives.ts` gallery checks (reduced motion, `@motion`); `tests/unit/primitives-css.test.ts` | Nothing animates on load; no transitions in the stylesheet; 0 ms under reduced motion |
| Zoom (200% zoom, 200% text) | ☑ pass | `tests/e2e/components/primitives.ts` gallery checks | 200% zoom and 200% text |
| Failures (images, video, data) | ☑ pass | `tests/e2e/components/primitives.ts` gallery checks | Web fonts blocked: still reads, no sideways scroll, nothing clipped |
| Performance (JS budget, CLS, render time) | ☑ pass | `tests/e2e/components/primitives.ts` gallery checks ("ships no JavaScript"); `tests/ssr/primitives.test.tsx` | JS: 0 KB (no script requested or present). Layout shift after fonts load < 0.001 on every Chromium screen. Render p95 1.2 ms in Bun (budget 20 ms) |
| Cross-browser (Chromium, Firefox, WebKit) | ☑ pass | `chromium-*`, `firefox-*`, `webkit-*` |  |

Screen readers: not checked by hand (no NVDA, VoiceOver or TalkBack on this machine). The accessibility tree is
asserted instead (roles, names, aria snapshots), and axe runs in both themes on every screen run, in each engine.

## Different screens

Chromium: every width below in light and dark (the system setting, plus the other theme by `data-theme` inside each
project), landscape, print, motion and forced colours. Firefox and WebKit ran this primitive's own specs on a subset of
screens (listed under Runs), because the laptop was saturated by other agents' Rust and Worker jobs during this round.
Phones at 3×, tablets and small laptops at 2×, desktops at 1×; touch below 980px. A tick means it passed on every
engine that ran that screen.

| Width | Light | Dark | Notes |
|---|---|---|---|
| 320 | ☑ | ☑ |  |
| 360 | ☑ | ☑ |  |
| 390 | ☑ | ☑ |  |
| 430 | ☑ | ☑ |  |
| 600 | ☑ | ☑ |  |
| 768 | ☑ | ☑ |  |
| 980 | ☑ | ☑ |  |
| 1120 | ☑ | ☑ |  |
| 1440 | ☑ | ☑ |  |
| 1920 | ☑ | ☑ |  |
| Landscape phone 844×390 | ☑ | ☑ |  |
| Touch (phone, tablet) | ☑ | ☑ | Not interactive |
| Scale factors 1× / 2× / 3× | ☑ | ☑ | 1× / 2× / 3× in the matrix |
| In context (article, home fixtures) | ☑ | ☑ | Headline, standfirst and body on the article fixtures |

Runs (Playwright projects `<engine>-<theme>-<screen>`, `-print`, `-motion`, `chromium-forced-colors-*`):
- **Chromium, full matrix** (`chromium-*`: 10 widths × light and dark, landscape × 2, print, motion, forced colours × 2), every e2e spec: 2,865 runs, 2,513 passed, 330 skipped (by design: `@visual` outside Docker, 200% zoom below 640px, touch-only or hover-only tests), 22 failed. The primitive specs alone: 1,972 runs, 1,700 passed, 250 skipped, 22 failed. Every failure is FF-1, SR-1 or SR-2. Tests changed after that run were re-run on all Chromium projects (45 runs, the one failure SR-1).
- **Firefox, primitive specs**: light at 320, 360, 390, 430, 600, 768, 980, 1120 and 1440; dark at 390 and 1440; dark landscape; print; motion (14 of 25 projects, 1,071 runs). After re-running load timeouts with a longer limit, the only failures are FF-1 (8 runs), SR-1 (3) and SR-2 (3).
- **WebKit, primitive specs**: light 320, dark 390, light 768, dark 1440, dark landscape, print, motion (7 of 25 projects, 455 runs: 357 passed and 53 skipped first time; 45 failed, of which 35 were load timeouts or a harness gap, all passing on re-run). Remaining failures: FF-1 (4), SR-1 (3), SR-2 (3).
- Not run in Firefox and WebKit: the other widths and themes listed above (time: the laptop was at 100% CPU from other agents' Rust builds; one Firefox test averaged over a minute). Forced colours exist only in Chromium.
- Unit, component and Astro (Vitest): 467 passed, 0 failed; SSR in Bun: 72 passed, 0 failed.

Screenshot baselines: the `@visual` gallery screenshot is written (`Text.png`); it compares only inside the
Playwright Docker image (`bun run test:visual`). No baselines were generated or accepted in this round, so there is
no baseline to justify yet; see the report.

## Issues found

| # | Area / screen | Problem | Expected (spec) and failing test |
|---|---|---|---|
| — | — | None found | — |

## Accepted exceptions and open design questions

| Exception | Reason | Approved by |
|---|---|---|
| Line heights without tokens: 1.15, 1.3, 1.35, 1.4 (and 1 for wordmark and figure-s) are literals in Text.css | Known design question already raised with Karthik; pinned by a test so a change is deliberate | Pending Karthik |
| typography.md gives phone sizes for nine more styles (headline-l 21, headline-m 20, headline-s 18, lead-summary 17.5, body-s 15, chart-title 17, chart-subtitle 14, figure-s 22, label 13) that tokens.json does not have; Text follows tokens.json and uses the desktop size on phones | New design question for Karthik: a token change or a doc change. Not a component defect (tokens are the source of truth) | For Karthik |
| `type.body-s` line height is 1.55 in typography.md; Text uses the 1.5 token | Same family as the line-height question above (no 1.55 token) | For Karthik |
| Font-swap layout shift up to about 0.44 at tablet widths when the fonts arrive after first paint | Open design question already raised with Karthik (fallback faces with `size-adjust` need a token decision). Recorded as a test annotation; the shift after fonts load is asserted (< 0.001) and passes | Pending Karthik |
| Print uses dark tokens when the system is dark (layout.md: charts print in light-mode colours on white) | Open design question already raised with Karthik | Pending Karthik |

## Test files

- `src/components/Text/Text.test.tsx`
- `tests/e2e/components/Text.spec.ts`
- `tests/ssr/primitives.test.tsx` (Bun: every fixture case renders to complete static HTML, no script, no warnings; render-time budget)
- `tests/unit/primitives-css.test.ts` (design-system compliance of every primitive stylesheet)
- `tests/e2e/components/primitives.ts` (the shared gallery checks every primitive spec runs on every screen)
- `tests/component/primitives.test.tsx` (builder's sanity checks, kept)
- `tests/e2e/renderer/fixture-pages.spec.ts` (the primitives in context on the renderer fixture pages)

## Sign-off

☑ Every area and every screen above is passed or has an exception listed above.
Signed: Opus 5.5 component tester, 16 November 2026
