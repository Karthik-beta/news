# Popover — test sign-off

Status: **not signed off**

Tester: Opus 5.5 component tester (not the builder) · Builder: Opus 5.5 component builder (issue #4) · Date: 16 November 2026
Branch / commit: `phase-1/site/renderer`, components at b842525, tests at 3092642 and the sign-off commit

## All areas

| Area | Result | Tests | Notes |
|---|---|---|---|
| Rendering | ☑ pass | `Popover.test.tsx` | Trigger is `type=button` with `popovertarget`; panel is `popover=auto`, `role=dialog`, named by `label`; extra trigger classes; rich triggers; phrasing content only; two popovers stay separate. Tooltip: placement and align classes, px and CSS lengths, negative values, no id by default, `role=status` |
| Data | ☑ n/a | — | Holds no data; SourceRef supplies it |
| Content edge cases | ☑ pass | `Popover.spec.ts`; fixture cases | Long content with an unbroken word wraps inside 320px; tooltips at the start, centre and end |
| Formatting | ☑ n/a | — | Formats nothing |
| Server rendering | ☑ pass | `tests/ssr/primitives.test.tsx`; hydration in the `.test.tsx` | Complete static HTML in plain Bun, no script, no hydration markers, no React warnings; server and client markup identical on hydration |
| Design-system compliance | ☑ pass | `tests/unit/primitives-css.test.ts`; `Popover.spec.ts` | Paper, 1px rule border, 12px padding, no shadow, radius 0, Fira Sans, at most 320px and inside the gutters, in both themes (components.md) |
| Accessibility | ☑ pass | `tests/e2e/components/primitives.ts` gallery checks; `Popover.test.tsx` | axe zero violations in both themes on every screen run in each engine |
| Interaction | ☑ pass | `Popover.spec.ts`; SourceRef's interaction tests | Click, Space, Enter open; Escape closes and returns focus; native light dismiss; no script. Hover/focus opening: known design question |
| Themes (light, dark ×2, forced colours, print) | ☒ fail | `Popover.spec.ts` | Light, dark and forced colours pass; print hides the open panel (correct) but also the trigger (SR-1) |
| Motion | ☑ pass | `Popover.spec.ts` (`@motion`); gallery checks | Appears instantly: no transition or animation on the panel |
| Zoom (200% zoom, 200% text) | ☑ pass | `tests/e2e/components/primitives.ts` gallery checks | 200% zoom and text pass; tooltip fixtures at fixed coordinates are left out of the 200%-text edge check because the owning island clamps them |
| Failures (images, video, data) | ☑ pass | `tests/e2e/components/primitives.ts` gallery checks | Web fonts blocked: still reads, no sideways scroll, nothing clipped |
| Performance (JS budget, CLS, render time) | ☑ pass | `tests/e2e/components/primitives.ts` gallery checks ("ships no JavaScript"); `tests/ssr/primitives.test.tsx` | JS: 0 KB (no script requested or present). Layout shift after fonts load < 0.001 on every Chromium screen. Render p95 2–4 ms in Bun (budget 20 ms) |
| Cross-browser (Chromium, Firefox, WebKit) | ☒ fail | `chromium-*`, `firefox-*`, `webkit-*` | SR-1 in every engine; SR-2 in all three |

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
| Landscape phone 844×390 | ☐ | ☐ | FAIL SR-2 (seen through SourceRef's popovers) |
| Touch (phone, tablet) | ☑ | ☑ | Opens on tap |
| Scale factors 1× / 2× / 3× | ☑ | ☑ | 1× / 2× / 3× in the matrix |
| In context (article, home fixtures) | ☑ | ☑ | Through SourceRef on the article fixtures |

Runs (Playwright projects `<engine>-<theme>-<screen>`, `-print`, `-motion`, `chromium-forced-colors-*`):
- **Chromium, full matrix** (`chromium-*`: 10 widths × light and dark, landscape × 2, print, motion, forced colours × 2), every e2e spec: 2,865 runs, 2,513 passed, 330 skipped (by design: `@visual` outside Docker, 200% zoom below 640px, touch-only or hover-only tests), 22 failed. The primitive specs alone: 1,972 runs, 1,700 passed, 250 skipped, 22 failed. Every failure is FF-1, SR-1 or SR-2. Tests changed after that run were re-run on all Chromium projects (45 runs, the one failure SR-1).
- **Firefox, primitive specs**: light at 320, 360, 390, 430, 600, 768, 980, 1120 and 1440; dark at 390 and 1440; dark landscape; print; motion (14 of 25 projects, 1,071 runs). After re-running load timeouts with a longer limit, the only failures are FF-1 (8 runs), SR-1 (3) and SR-2 (3).
- **WebKit, primitive specs**: light 320, dark 390, light 768, dark 1440, dark landscape, print, motion (7 of 25 projects, 455 runs: 357 passed and 53 skipped first time; 45 failed, of which 35 were load timeouts or a harness gap, all passing on re-run). Remaining failures: FF-1 (4), SR-1 (3), SR-2 (3).
- Not run in Firefox and WebKit: the other widths and themes listed above (time: the laptop was at 100% CPU from other agents' Rust builds; one Firefox test averaged over a minute). Forced colours exist only in Chromium.
- Unit, component and Astro (Vitest): 467 passed, 0 failed; SSR in Bun: 72 passed, 0 failed.

Screenshot baselines: the `@visual` gallery screenshot is written (`Popover.png`); it compares only inside the
Playwright Docker image (`bun run test:visual`). No baselines were generated or accepted in this round, so there is
no baseline to justify yet; see the report.

## Issues found

| # | Area / screen | Problem | Expected (spec) and failing test |
|---|---|---|---|
| SR-1 | Print, all engines | The trigger is hidden in print by the base `button { display: none }` rule, so the cited figure or term disappears from the sentence | Print keeps the text and drops only the panel. Failing test: `tests/e2e/components/Popover.spec.ts` "prints the trigger's text (the figure or term) in its sentence" (and SourceRef's print tests) |
| SR-2 | Landscape phone 844×390, all three engines | No position fallback fits: Chromium and WebKit overflow the bottom of the screen by 18px; Firefox covers 14px of the trigger | Wholly on screen. Failing tests: SourceRef.spec.ts popover placement tests on `*-landscape` |

## Accepted exceptions and open design questions

| Exception | Reason | Approved by |
|---|---|---|
| Opens on click, tap, Enter and Space, not hover or focus | Known design question already raised with Karthik | Pending Karthik |
| On a 390px-wide phone the panel can touch the screen edge (its width is capped to the gutters, but the anchored position may start or end at the edge) | Observation, not a spec breach (max 320px wide is met); worth a margin when SR-2 is fixed | Not a blocker |
| Font-swap layout shift up to about 0.44 at tablet widths when the fonts arrive after first paint | Open design question already raised with Karthik (fallback faces with `size-adjust` need a token decision). Recorded as a test annotation; the shift after fonts load is asserted (< 0.001) and passes | Pending Karthik |
| Print uses dark tokens when the system is dark (layout.md: charts print in light-mode colours on white) | Open design question already raised with Karthik | Pending Karthik |

## Test files

- `src/components/Popover/Popover.test.tsx`
- `tests/e2e/components/Popover.spec.ts`
- Fixture cases added: "tooltip below, aligned to the end", "tooltip below, aligned to the start"
- `tests/ssr/primitives.test.tsx` (Bun: every fixture case renders to complete static HTML, no script, no warnings; render-time budget)
- `tests/unit/primitives-css.test.ts` (design-system compliance of every primitive stylesheet)
- `tests/e2e/components/primitives.ts` (the shared gallery checks every primitive spec runs on every screen)
- `tests/component/primitives.test.tsx` (builder's sanity checks, kept)
- `tests/e2e/renderer/fixture-pages.spec.ts` (the primitives in context on the renderer fixture pages)

## Sign-off

☐ **Not signed off.** Blocking: SR-1 (the trigger, and with it the cited figure or term, is hidden in print) and SR-2 (placement off screen on a landscape phone). SR-1 comes from the print rules in `src/styles/base.css` hiding every button, which hides Popover's trigger; SR-2 is Popover.css's anchor positioning. See SourceRef's sign-off for the full defect reports.
Re-test after the fix: the failing tests named in Issues found, on the screens named there, then the full matrix for this primitive. Tester: Opus 5.5 component tester, 16 November 2026
