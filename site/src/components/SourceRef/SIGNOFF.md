# SourceRef — test sign-off

Status: **not signed off**

Tester: Opus 5.5 component tester (not the builder) · Builder: Opus 5.5 component builder (issue #4) · Date: 16 November 2026
Branch / commit: `phase-1/site/renderer`, components at b842525, tests at 3092642 and the sign-off commit

## All areas

| Area | Result | Tests | Notes |
|---|---|---|---|
| Rendering | ☑ pass | `SourceRef.format.test.tsx`; `tests/component/primitives.test.tsx` | Compact and as-reported styles, `currencyNamed`, `className`, missing state, RefText with `popover` and `plain`, empty and token-free text |
| Data | ☑ pass | `SourceRef.format.test.tsx` | dp_ and dv_ references resolve and record their sources; null, unknown, unknown dv_ and wrong-kind references say "No data" (never a digit), are not buttons and report the problem; zero, negative, ≥ 1e12 and very small values; derived values cite every input's source |
| Content edge cases | ☑ pass | `SourceRef.spec.ts`; fixture cases | Figure inside a heading and at the right end of a line; long popover content wraps |
| Formatting | ☑ pass | `SourceRef.format.test.tsx` (51 cases) | $, €, £, ¥ (only when named), ₹, ₩, NT$, HK$, A$, C$, S$, CHF; bn/m/trn; 999.96bn→1.0trn; no added precision; true minus before the currency; thousands separators; %, points, basis points; dates (1 November 2024; date-times as their UTC day); fiscal labels verbatim (Q4 FY26, fourth quarter of fiscal 2024) |
| Server rendering | ☑ pass | `tests/ssr/primitives.test.tsx`; hydration in the `.test.tsx` | Complete static HTML in plain Bun, no script, no hydration markers, no React warnings; server and client markup identical on hydration |
| Design-system compliance | ☑ pass | `tests/unit/primitives-css.test.ts`; `SourceRef.spec.ts` | Tokens only; the figure keeps the sentence's face, size and colour with a dotted 1px ink-3 underline, offset 4px (typography.md); missing data in ink-2; no crimson |
| Accessibility | ☑ pass | `tests/e2e/components/primitives.ts` gallery checks; `SourceRef.spec.ts` | axe zero violations in both themes, in every engine and screen run, open and closed; the popover is a named dialog ("Source for $391.0bn"); visible 2px focus ring; stand-alone key-figure triggers ≥ 24×24 (figures in a sentence use the WCAG inline exception) |
| Interaction | ☑ pass | `SourceRef.spec.ts` | Click, tap, Enter and Space open; Tab reaches "Open the source"; Escape closes and returns focus; a click or tap outside closes; opening another closes the first; opening never moves the text. Hover and focus do not open it: the known design question, recorded |
| Themes (light, dark ×2, forced colours, print) | ☒ fail | `SourceRef.spec.ts`; gallery checks | Light and dark (system and `data-theme`) pass, popover paper and rule border from the tokens; forced colours pass; print fails: SR-1 |
| Motion | ☑ pass | `tests/e2e/components/primitives.ts` gallery checks (reduced motion, `@motion`); `tests/unit/primitives-css.test.ts` | Nothing animates on load; no transitions in the stylesheet; 0 ms under reduced motion |
| Zoom (200% zoom, 200% text) | ☑ pass | `tests/e2e/components/primitives.ts` gallery checks | 200% zoom and 200% text: no sideways scroll, nothing clipped |
| Failures (images, video, data) | ☑ pass | `tests/e2e/components/primitives.ts` gallery checks; `SourceRef.format.test.tsx` | Web fonts blocked: still reads, no sideways scroll, nothing clipped; missing data renders "No data" |
| Performance (JS budget, CLS, render time) | ☑ pass | `tests/e2e/components/primitives.ts` gallery checks ("ships no JavaScript"); `tests/ssr/primitives.test.tsx` | JS: 0 KB (no script requested or present). Layout shift after fonts load < 0.001 on every Chromium screen. Render p95 4–10 ms in Bun (budget 20 ms) |
| Cross-browser (Chromium, Firefox, WebKit) | ☒ fail | `chromium-*`, `firefox-*`, `webkit-*` | SR-1 in every engine's print project; SR-2 in all three (Chromium and WebKit overflow the screen, Firefox covers the figure) |

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
| Landscape phone 844×390 | ☐ | ☐ | FAIL SR-2 (Chromium, WebKit: popover 18px below the screen; Firefox: covers the figure) |
| Touch (phone, tablet) | ☑ | ☑ | Opens on tap, closes on a tap outside |
| Scale factors 1× / 2× / 3× | ☑ | ☑ | 1× / 2× / 3× in the matrix |
| In context (article, home fixtures) | ☑ | ☑ | Popovers in the Apple and static article fixtures open on screen at every width; print fails there too (SR-1). No home page fixture yet |

Runs (Playwright projects `<engine>-<theme>-<screen>`, `-print`, `-motion`, `chromium-forced-colors-*`):
- **Chromium, full matrix** (`chromium-*`: 10 widths × light and dark, landscape × 2, print, motion, forced colours × 2), every e2e spec: 2,865 runs, 2,513 passed, 330 skipped (by design: `@visual` outside Docker, 200% zoom below 640px, touch-only or hover-only tests), 22 failed. The primitive specs alone: 1,972 runs, 1,700 passed, 250 skipped, 22 failed. Every failure is FF-1, SR-1 or SR-2. Tests changed after that run were re-run on all Chromium projects (45 runs, the one failure SR-1).
- **Firefox, primitive specs**: light at 320, 360, 390, 430, 600, 768, 980, 1120 and 1440; dark at 390 and 1440; dark landscape; print; motion (14 of 25 projects, 1,071 runs). After re-running load timeouts with a longer limit, the only failures are FF-1 (8 runs), SR-1 (3) and SR-2 (3).
- **WebKit, primitive specs**: light 320, dark 390, light 768, dark 1440, dark landscape, print, motion (7 of 25 projects, 455 runs: 357 passed and 53 skipped first time; 45 failed, of which 35 were load timeouts or a harness gap, all passing on re-run). Remaining failures: FF-1 (4), SR-1 (3), SR-2 (3).
- Not run in Firefox and WebKit: the other widths and themes listed above (time: the laptop was at 100% CPU from other agents' Rust builds; one Firefox test averaged over a minute). Forced colours exist only in Chromium.
- Unit, component and Astro (Vitest): 467 passed, 0 failed; SSR in Bun: 72 passed, 0 failed.

Screenshot baselines: the `@visual` gallery screenshot is written (`SourceRef.png`); it compares only inside the
Playwright Docker image (`bun run test:visual`). No baselines were generated or accepted in this round, so there is
no baseline to justify yet; see the report.

## Issues found

| # | Area / screen | Problem | Expected (spec) and failing test |
|---|---|---|---|
| SR-1 | Themes: print / `*-print` (all three engines) | Every cited figure disappears when the page is printed: "Net sales were  in fiscal 2024." The print rules in `src/styles/base.css` hide every `button` (`display: none !important`), and the figure is the popover's trigger button | layout.md, Print: keep the head, figures and prose; only interface parts and the popover are dropped. Failing tests: `tests/e2e/components/SourceRef.spec.ts` "prints every cited figure as text and no popover" and "in context: prints the article's cited figures"; `Popover.spec.ts` "prints the trigger's text" |
| SR-2 | Interaction / landscape phone 844×390, light and dark, all three engines | A popover opened from a figure in the middle of a short screen has no placement that fits (191px panel; 177px below, 184px above). Chromium and WebKit place it below anyway, so its last 18px, including part of "Open the source", are below the screen; Firefox shifts it up over the figure, covering 14px of the trigger | components.md and accessibility.md: the popover shows wholly on screen at every size. Failing tests: `SourceRef.spec.ts` "a click opens the citation popover…", "every popover on the page opens on screen…" and "in context: popovers in the article prose and key figures open on screen" on `*-landscape`. The position rules are Popover's (`Popover.css`) |

## Accepted exceptions and open design questions

| Exception | Reason | Approved by |
|---|---|---|
| Popover opens on click, tap, Enter and Space, not on hover or focus (components.md says hover, focus or tap) | Known design question already raised with Karthik; recorded by the test "hover does not open the popover" | Pending Karthik |
| Compact values in units the house style does not know drop the unit on the page ("0.000000012" for mol/L); the popover writes it ("0.000000012 mol/L") | Builder's documented choice (the unit appears once, in the subtitle or the sentence); worth confirming for prose | Not a blocker; for Karthik |
| Font-swap layout shift up to about 0.44 at tablet widths when the fonts arrive after first paint | Open design question already raised with Karthik (fallback faces with `size-adjust` need a token decision). Recorded as a test annotation; the shift after fonts load is asserted (< 0.001) and passes | Pending Karthik |
| Print uses dark tokens when the system is dark (layout.md: charts print in light-mode colours on white) | Open design question already raised with Karthik | Pending Karthik |

## Test files

- `src/components/SourceRef/SourceRef.format.test.tsx` (formatting, data, popover content, missing data, RefText)
- `tests/e2e/components/SourceRef.spec.ts` (values, look, click, keyboard, tap, light dismiss, placement on every screen, print, in context)
- `tests/support/data.ts` (synthetic data through the real schema)
- Fixture cases added: "cited figure at the end of a right-aligned line", "cited figure inside a heading"
- `tests/ssr/primitives.test.tsx` (Bun: every fixture case renders to complete static HTML, no script, no warnings; render-time budget)
- `tests/unit/primitives-css.test.ts` (design-system compliance of every primitive stylesheet)
- `tests/e2e/components/primitives.ts` (the shared gallery checks every primitive spec runs on every screen)
- `tests/component/primitives.test.tsx` (builder's sanity checks, kept)
- `tests/e2e/renderer/fixture-pages.spec.ts` (the primitives in context on the renderer fixture pages)

## Sign-off

☐ **Not signed off.** Blocking: SR-1 (cited figures vanish in print) and SR-2 (popover partly off screen on a landscape phone).
Re-test after the fix: the failing tests named in Issues found, on the screens named there, then the full matrix for this primitive. Tester: Opus 5.5 component tester, 16 November 2026
