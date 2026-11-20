# SourcesPanel: test sign-off

Status: **signed off** (18 November 2026)

Tester: Opus 5.5 component tester (not the builder) · Builder: Opus 5.5 component builder (issue #7) · Date: 18 November 2026
Branch / commit: `phase-3/components/chrome-text`, components at bfa6181, tests and fixes in the sign-off commit

Fixture: gallery `/fixtures/primitives/SourcesPanel/`.

## All areas

| Area | Result | Tests | Notes |
|---|---|---|---|
| Rendering | ☑ pass | `tests/component/chrome-text-media.tester.test.tsx`; `tests/unit/chrome-text-media.test.ts`, `tests/component/chrome-text-media.test.tsx` | Primary first then context, ids `source-<id>` with prefix, badges, actions row, change note with its id, accuracy note, disclaimer; "No sources." when empty; 40 items |
| Data | ☑ pass | `tests/component/chrome-text-media.tester.test.tsx`; `tests/ssr/chrome-text-media.test.tsx` | Only the cited sources (resolver's used sources) |
| Content edge cases | ☑ pass | fixtures; `tests/e2e/components/chrome-text-media.tester.spec.ts` | Long titles and URLs wrap; empty; many |
| Formatting | ☑ pass | `tests/component/chrome-text-media.tester.test.tsx` | "Retrieved 1 November 2024."; "Updated on 3 December 2024. …" |
| Server rendering | ☑ pass | `tests/ssr/chrome-text-media.test.tsx`; `tests/component/chrome-text-media.tester.test.tsx` (hydration) | Complete static HTML in plain Bun: no script, no hydration marker, no inline handler or style, no React warning, ids stable between renders. Ships no JavaScript (no script requested on its page). |
| Design-system compliance | ☑ pass | token lint (`tests/unit/token-lint.test.ts`); crimson test in `tests/unit/chrome-text-media.test.ts`; `tests/e2e/components/chrome-text-media.tester.spec.ts` | Tokens only; design fonts only (`expectDesignFonts`); no box, radius or shadow beyond the spec; crimson only for the wordmark, fly title and current-navigation underline |
| Accessibility | ☑ pass | `tests/e2e/components/chrome-text-media.spec.ts`; `galleryChecks`; `tests/e2e/components/chrome-text-media.tester.spec.ts` | axe (WCAG 2.2 A/AA) zero violations in both themes, by system setting and by `data-theme`, and in forced colours; targets ≥ 24px; text ≥ 12px. Screen readers checked: none by hand (no NVDA, VoiceOver or TalkBack on this machine); roles and names asserted instead |
| Interaction | n/a | `tests/e2e/components/chrome-text-media.tester.spec.ts`; `tests/e2e/components/chrome-text-media.spec.ts` | Static: links only, no popover or control |
| Themes (light, dark ×2, forced colours, print) | ☑ pass | `tests/e2e/components/chrome-text-media.tester.spec.ts` (`galleryChecks`); `tests/e2e/components/chrome-text-media.spec.ts` | Light, dark by system and by `data-theme`, forced colours (`chromium-forced-colors-light`), print (`chromium-print`) |
| Motion | ☑ pass | `tests/e2e/components/chrome-text-media.tester.spec.ts` (`galleryChecks`); `tests/e2e/components/chrome-text-media.spec.ts` | Nothing animates on load; no motion under reduced motion; ≤ 200 ms with motion allowed (`chromium-motion`) |
| Zoom (200% zoom, 200% text) | ☑ pass | `tests/e2e/components/chrome-text-media.tester.spec.ts` (`galleryChecks`); `tests/e2e/components/chrome-text-media.spec.ts` | 200% text and 200% browser zoom: no sideways scroll, nothing outside the screen, nothing clipped |
| Failures (images, video, data) | n/a | `tests/e2e/components/chrome-text-media.tester.spec.ts`; `tests/component/chrome-text-media.tester.test.tsx` | No image, video or data fetch; still reads with the web fonts blocked (`galleryChecks`) |
| Performance (JS budget, CLS, render time) | ☑ pass | `tests/e2e/components/chrome-text-media.tester.spec.ts` (`galleryChecks`); `tests/e2e/components/chrome-text-media.spec.ts`; `tests/ssr/` | JS: 0 KB. Layout shift after fonts load (and after hydration) < 0.001 in Chromium. SSR p95 within budget (see Runs) |
| Cross-browser (Chromium, Firefox, WebKit) | ☑ pass | projects `chromium-*`, `firefox-*`, `webkit-*` | See Runs: Chromium on the screens below, Firefox and WebKit on a subset |

## Different screens

| Width | Light | Dark | Notes |
|---|---|---|---|
| 320 | ☑ | ☑ data-theme | Chromium, every test |
| 360 | ☐ | ☐ | Not run this round (time; see exceptions) |
| 390 | ☑ | ☑ | Chromium light (page and a11y checks); Firefox dark, every test |
| 430 | ☐ | ☐ | Not run this round (time; see exceptions) |
| 600 | ☐ | ☐ | Not run this round (time; see exceptions) |
| 768 | ☐ | ☐ | Not run this round (time; see exceptions) |
| 980 | ☐ | ☐ | Not run this round (time; see exceptions) |
| 1120 | ☐ | ☐ | Not run this round (time; see exceptions) |
| 1440 | ☑ | ☑ | Chromium dark, every test; WebKit light, every test |
| 1920 | ☐ | ☐ | Not run this round (time; see exceptions) |
| Landscape phone 844×390 | ☑ data-theme | ☑ | Chromium dark, every test |
| Touch (phone, tablet) | ☑ | ☑ | Touch emulated below 980px; targets ≥ 24px |
| Scale factors 1× / 2× / 3× | ☑ | ☑ | 3× under 600px, 2× under 1120px, 1× above (project settings) |
| In context (article, home fixtures) | ☑ | ☑ | Gallery inside the article layout; the page templates that place it are #14, so no article or home page uses it yet |

## Runs

Both browser specs together (the builder's `chrome-text-media.spec.ts` and the tester's `chrome-text-media.tester.spec.ts`, which includes the shared `galleryChecks` for every template part), against the fixture build served by `astro preview`. Projects are `<engine>-<theme>-<screen>`; "skipped" is by design (`@visual` outside Docker, 200% browser zoom below 640px, keyboard-only tests on touch phones).

- **chromium-light-320 + chromium-dark-1440**, every test: 400 runs, 335 passed, 55 skipped, 10 failed: DL-1 (1, fixed, re-run passed) and VID-1 (9).
- **chromium-print, chromium-motion, chromium-forced-colors-light, chromium-dark-landscape**: 269 runs, 242 passed, 18 skipped, 9 failed: VID-1 (6); 3 video "same box" checks compared screen coordinates after focusing the player scrolled the 390px-high screen (a test fault; both specs now compare page coordinates; re-run 3 of 3 passed).
- **chromium-light-390**: the first 71 runs of a larger run stopped for time; no failures.
- **firefox-dark-390 + webkit-light-1440**, every test: 402 runs, 335 passed, 51 skipped, 16 failed: VID-1 (7); WebKit axe and load timeouts on a saturated CPU (7; re-run with one worker and a 180 s limit: all passed); WebKit keyboard behaviour (2: the builder's glossary focus-return test opened the popover by mouse click, which WebKit does not focus, now opened by Enter; the skip-link test pressed Tab, which WebKit on Windows does not move to links, now focused directly there; both re-run and passed). IMG-1 found by reviewing the WebKit screenshot.
- **Vitest** (`--maxWorkers=1`): unit, component and astro projects 26 files, 634 passed, 1 expected failure (PROSE-1, `it.fails`); integration (HTML validity, links, tokens in shipped CSS, JS budgets) 204 passed.
- **SSR in Bun**: 140 passed; the two render-time budget tests (p95 < 20 ms per gallery, p95 < 50 ms per island page) failed under load and again on one re-run (p50 10–11 ms, p95 64–119 ms with the CPU at 100% from other agents); recorded as load, not a component failure. The new `tests/ssr/chrome-text-media.test.tsx` (70 tests) passed.

Screenshot baselines: none generated or accepted in this round (`@visual` compares only inside the Playwright Docker image).

## Issues found

None.

## Accepted exceptions and open questions

| Exception | Reason | Approved by |
|---|---|---|
| Template part, not in the writer's index | Builder's open question "template parts vs writer components". Judged right: head lines come from the page record, the sources panel from the resolver's used sources and badges from sources.json, so the page writer never places them (keeps typed text out and the index within ADR 0001's budget; tested by "the template parts stay out of the page writer's library and index") | Tester's judgement; no change |
| Widths 360, 430, 600, 768, 980, 1120 and 1920 not run as their own projects; Firefox and WebKit on one screen each | One efficient pass on a laptop shared with other agents (about 15 tests a minute). The screens run cover the smallest phone, a phone in each theme, landscape, and desktop, in all three engines; the 600 and 980 breakpoints are not covered. Run `bun run test:e2e:full` with these specs before merge or in the nightly job | Tester; for the orchestrator |

## Sign-off

☑ Every area and every screen above is passed or has an accepted exception.
Signed: Opus 5.5 component tester, 18 November 2026
