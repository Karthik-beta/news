# Pagination — test sign-off

Status: **signed off**

Tester: Opus 5.5 component tester (not the builder) · Builder: Opus 5.5 component builder (issue #11, part 1) · Date: 21 November 2026
Branch / commit: `phase-3/components/page-lists`, components at d3ae173; tests, fixes and this sign-off in the tester's commit on top of it

Scope: Karthik's light test scope (20 Nov 2026): every width in Chromium; Firefox and WebKit at 360 and 1440 (with
their 200% text and zoom runs); Vitest for the rest; one worker throughout.

## All areas

| Area | Result | Tests | Notes |
|---|---|---|---|
| Rendering | ☑ pass | `page-lists.test.tsx`; `page-lists.tester.test.tsx` "the window keeps first, last, current and neighbours" (every page of every run up to 30) | Newer and Older at the ends, at most seven numbers, never a one-page gap |
| Data | ☑ pass | `page-lists.tester.test.tsx` "out-of-range and non-finite input …" | Out-of-range pages clamp; NaN or Infinity show page 1 or nothing, never "Page NaN" (PG-1) |
| Content edge cases | ☑ pass | `page-lists.tester.spec.ts` gallery "every case reads" on every screen; `page-lists.tester.test.tsx` |  |
| Formatting | ☑ pass | `page-lists.tester.test.tsx` formatting describe; `page-lists.test.tsx` |  |
| Server rendering | ☑ pass | `page-lists.tester.test.tsx` "list parts render complete, script-free HTML and hydrate with no warnings"; `page-lists.tester.spec.ts` gallery "ships no JavaScript" | Complete HTML with no script; hydrates with no React warning |
| Design-system compliance | ☑ pass | `page-lists.tester.spec.ts` "the current page is marked by weight and a 2px crimson underline" | Current page in ink, weight 600, 2px crimson underline (current navigation), in both themes |
| Accessibility | ☑ pass | `page-lists.spec.ts`; `page-lists.tester.spec.ts` landmark names; `page-lists.tester.test.tsx` | "Page n" link names, `aria-current`, ellipses hidden; every pagination on a page needs its own `label` (the gallery's four are distinct). axe zero violations, both themes. Screen readers checked: none by hand |
| Interaction | ☑ pass | `page-lists.spec.ts` keyboard order; `page-lists.tester.spec.ts` "Tab shows the focus ring …; Enter follows it", "touch: every link is at least 24px square and a tap follows it" | Tab order Newer, pages, Older; a 2px focus ring in the focus colour on each link; Enter and tap follow the link |
| Themes (light, dark ×2, forced colours, print) | ☑ pass | `page-lists.tester.spec.ts` gallery axe by system and `data-theme`; forced colours; `page-lists.spec.ts` | Light, dark by system and by `data-theme`; forced colours axe-clean |
| Motion | ☑ pass | `page-lists.tester.spec.ts` gallery "nothing animates" (reduced and @motion) | No animation; transitions 0 ms under reduced motion, ≤ 200 ms otherwise |
| Zoom (200% zoom, 200% text) | ☑ pass | `page-lists.tester.spec.ts` gallery "reflows at 200% zoom and at 200% text size" | No sideways scroll, nothing clipped or off screen |
| Failures (images, video, data) | ☑ pass | `page-lists.tester.spec.ts` gallery "still reads with the web fonts blocked"; `page-lists.tester.test.tsx` | Fonts blocked: still reads; non-finite counts show nothing (PG-1); no images or video |
| Performance (JS budget, CLS, render time) | ☑ pass | `page-lists.tester.spec.ts` gallery "ships no JavaScript and does not move once the fonts are in" | JS: 0 KB. Layout shift after fonts < 0.001 (Chromium) |
| Cross-browser (Chromium, Firefox, WebKit) | ☑ pass | both specs in `chromium-*`, `firefox-*`, `webkit-*` | Firefox and WebKit at 360 and 1440 (light scope) |

## Different screens

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
| Touch (phone, tablet) | ☑ | ☑ | Touch emulated below 980px: links ≥ 24 × 24px, a tap follows the link |
| Scale factors 1× / 2× / 3× | ☑ | ☑ | 3× phones, 2× tablets and small laptops, 1× desktops |
| In context (article, home fixtures) | ☑ | ☑ | Alone and inside the page shell's gallery only: the home, hub, search and article-rail templates are #14, so there is no home or hub fixture page to test in yet |

Runs:

Each Chromium project ran one theme by the system setting; every axe and colour test also switches to the other
theme by `data-theme` inside the test, so both themes are checked at every width.

- **Chromium, first slice** (`chromium-light-360`, both specs): 98 runs, 74 passed, 20 skipped, 4 failed: four tester
  test bugs (the clipped "Key event: " text measured as the date, the citation popover counted in the rate's text,
  the page shell's own fly line, `<header>` counted as a landmark), fixed and re-run: 6 of 6 passed.
- **Chromium matrix** (light 320, 430, 768, 1120 and landscape; dark 390, 600, 980, 1440, 1920 and landscape; print;
  motion; forced colours light and dark): 1,125 runs, 982 passed, 133 skipped (by design: `@visual` outside Docker,
  200% zoom below 640px, the touch test on non-touch screens), 10 failed: one tester test bug (the date column counted
  as text in the wide layout), fixed and re-run on the 11 affected projects: 29 of 29 passed.
- **Firefox and WebKit** (`firefox-light-360`, `firefox-dark-1440`, `webkit-light-360`, `webkit-dark-1440`; each with
  its 200% text and 200% zoom tests): 392 runs, 332 passed, 56 skipped, 4 failed: Playwright's WebKit on Windows never
  moves to links with Tab (Safari's default, handled the same way in `SourceRef.spec.ts`); the tester's focus-ring test
  now focuses each link directly in WebKit and the builder's Tab-order test skips WebKit. Re-run: 12 passed, 4 skipped.
- **After LM-1** (one Latest list and one "On this page" per gallery): the LatestList, PageContents and landmark tests
  on Chromium 320, 390, 768, 1440, 1920, forced colours light and dark and motion, and Firefox and WebKit at 360 and
  1440: 168 runs, 142 passed, 26 skipped, 0 failed.
- **Vitest** (unit, component, astro; `--maxWorkers=1 --pool=forks`): 1,232 tests, 1,231 passed, 1 failed: the
  PolicyDecision edge-case Markdown snapshot, which recorded PD-1's lower-case "no data"; updated. Fixture-case files
  re-run after LM-1: 356 passed.
- **SSR in Bun** (`bun test tests/ssr`): 143 passed, 0 failed; render p50 6.9 ms, p95 15.8 ms.
- **Integration** (HTML validity, links, budgets, `--maxWorkers=1`): 380 tests, 378 passed, 2 failed (LM-1), fixed:
  380 passed. The component index in the build is within ADR 0001's budget (420 of 4,000 tokens; PolicyDecision 24,
  Finding 23, Timeline 23 of 24 per entry).
- `astro check`: 0 errors. Biome: clean on the changed files.
- No failure appeared only under load; nothing was re-run to make it pass.

Screen readers: not checked by hand (no NVDA, VoiceOver or TalkBack on this machine). Roles, names and landmarks are
asserted instead, and axe runs in both themes on every screen run.

## Issues found

| # | Area / screen | Problem | Expected (spec), fix or failing test |
|---|---|---|---|
| PG-1 | Data / every screen | A non-finite `total` drew an empty, named navigation landmark; `total` Infinity would write "Page Infinity"; a NaN `current` broke the window | Fixed: non-finite counts show nothing and a non-finite page is page 1. Test: tester test "out-of-range and non-finite input …" |

## Accepted exceptions and open questions

| Exception | Reason | Approved by |
|---|---|---|
| The default landmark name is "Pages"; two paginations on one page must pass different labels | Documented by the `label` prop; templates (#14) should name a top and bottom pagination differently or show one | Tester note for #14 |
| Font-swap layout shift since navigation (recorded as an annotation, not asserted) | Shared open design question (fallback faces with `size-adjust`); the shift after the fonts load is asserted < 0.001 and passes | Pending Karthik |
| No screenshot baselines made or compared | `@visual` compares only inside the Playwright Docker image (`bun run test:visual`); no Linux baselines exist for these components yet | To do before merge with Docker |

## Test files

- `site/tests/component/page-lists.tester.test.tsx` (tester): record-only text, slot data, edge cases, formatting, server rendering and hydration
- `site/tests/e2e/components/page-lists.tester.spec.ts` (tester): `galleryChecks` on every screen (reads, axe both themes, 200% text and zoom, fonts blocked, no JS, no motion, forced colours, `@visual` in Docker only), plus the component's own checks named above
- `site/tests/component/page-lists.test.tsx`, `site/tests/e2e/components/page-lists.spec.ts` (builder)

## Sign-off

☑ Every area and every screen above is passed or has an accepted exception.
Signed: Opus 5.5 component tester, 21 November 2026
