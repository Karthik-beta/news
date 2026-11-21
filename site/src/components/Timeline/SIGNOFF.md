# Timeline — test sign-off

Status: **signed off, with an open question for Karthik**

Tester: Opus 5.5 component tester (not the builder) · Builder: Opus 5.5 component builder (issue #11, part 1) · Date: 21 November 2026
Branch / commit: `phase-3/components/page-lists`, components at d3ae173; tests, fixes and this sign-off in the tester's commit on top of it

Scope: Karthik's light test scope (20 Nov 2026): every width in Chromium; Firefox and WebKit at 360 and 1440 (with
their 200% text and zoom runs); Vitest for the rest; one worker throughout.

## All areas

| Area | Result | Tests | Notes |
|---|---|---|---|
| Rendering | ☑ pass | `page-lists.test.tsx`; `page-lists.tester.test.tsx` "one key event at most" | One key event, in words ("Key event: ") and by its ring |
| Data | ☑ pass | `page-lists.test.tsx` "say No data … never zero"; `page-lists.tester.test.tsx` |  |
| Content edge cases | ☑ pass | `page-lists.tester.spec.ts` "nothing clipped or off screen" on every screen; edge-cases fixture |  |
| Formatting | ☑ pass | `page-lists.tester.test.tsx`; `page-lists.test.tsx` | Period labels exactly as the source writes them ("fiscal 2024", "Early 2020s, as the source puts it"); `<time>` only when the period has a start date |
| Server rendering | ☑ pass | `page-lists.tester.test.tsx` "the page types render with no script"; `page-lists.spec.ts` "static parts ship no JavaScript" |  |
| Design-system compliance | ☑ pass | `page-lists.tester.spec.ts` "the key dot is crimson with a 1px ring 2px out", "dots on the line" | 8px ink dots on a 1px rule line; the key dot crimson with a 1px ring 2px out, in both themes |
| Accessibility | ☑ pass | `page-lists.spec.ts` axe both themes on every screen; `page-lists.tester.spec.ts` forced colours axe | axe: zero violations of any impact, light and dark. Screen readers checked: none by hand (see Runs) |
| Interaction | ☑ pass | `page-lists.spec.ts`; data-charts specs (SourcePopover) | Cited values open their citation popover (SourcePopover, signed off in #8); nothing else is interactive |
| Themes (light, dark ×2, forced colours, print) | ☑ pass | `page-lists.tester.spec.ts` forced colours (dots in system colours, key ring kept), print (dots print exact) | Forced colours: dots in CanvasText, the key dot in Highlight with its ring |
| Motion | ☑ pass | `page-lists.tester.spec.ts` "nothing animates on load", "transitions stay within 200 ms" (@motion) | No animation on load; 0 ms under reduced motion |
| Zoom (200% zoom, 200% text) | ☑ pass | `page-lists.tester.spec.ts` "dots on the line … at 100% and 200% text, and at 200% zoom"; "reflows" | Dots stay centred on the line (≤ 0.75px) and level with the date's first line (≤ 1.5px) at 100% and 200% text and 200% zoom; the line has no gap and ends at the first and last dots |
| Failures (images, video, data) | ☑ pass | `page-lists.test.tsx`; `page-lists.tester.test.tsx` | Unknown references say "No data"; no images or video |
| Performance (JS budget, CLS, render time) | ☑ pass | `page-lists.spec.ts` "static parts ship no JavaScript"; `page-lists.tester.spec.ts` layout shift | JS: 0 KB. Layout shift after fonts < 0.001 (Chromium) |
| Cross-browser (Chromium, Firefox, WebKit) | ☑ pass | both specs in `chromium-*`, `firefox-*`, `webkit-*` | Firefox and WebKit at 360 and 1440 (light scope) |

## Different screens

| Width | Light | Dark | Notes |
|---|---|---|---|
| 320 | ☑ | ☑ |  |
| 360 | ☑ | ☑ |  |
| 390 | ☑ | ☑ |  |
| 430 | ☑ | ☑ |  |
| 600 | ☑ | ☑ | Date moves into its 7em column from here |
| 768 | ☑ | ☑ |  |
| 980 | ☑ | ☑ |  |
| 1120 | ☑ | ☑ |  |
| 1440 | ☑ | ☑ |  |
| 1920 | ☑ | ☑ |  |
| Landscape phone 844×390 | ☑ | ☑ |  |
| Touch (phone, tablet) | ☑ | ☑ | Touch emulated below 980px; targets ≥ 24px |
| Scale factors 1× / 2× / 3× | ☑ | ☑ | 3× phones, 2× tablets and small laptops, 1× desktops |
| In context (article, home fixtures) | ☑ | ☑ | Component fixture pages render inside the article page shell (head, body, sources panel): blocks ≥ 16px apart, in order, axe-clean in both themes. No real article template uses it yet (#14) |

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
| TL-1 | Rendering / every screen | Two events on the highlighted date were both marked key (two crimson dots, two "Key event"), breaking "one key event" | Fixed: only the first event on that date is the key one, in the page and the Markdown. Test: tester test "one key event at most" |

## Accepted exceptions and open questions

| Exception | Reason | Approved by |
|---|---|---|
| The key event's dot is crimson even when the key event is a fall (the default fixture's key event is "The first cut") | Karthik's rule (16 Nov 2026) is written for a chart's highlighted data subject, decided from the data; a dated event has no direction the component can read. Question for Karthik: should a timeline's key dot use `chart.neutral-highlight` when the writer marks a fall, or does the rule not apply to events? | Open: Karthik |
| Font-swap layout shift since navigation (recorded as an annotation, not asserted) | Shared open design question (fallback faces with `size-adjust`); the shift after the fonts load is asserted < 0.001 and passes | Pending Karthik |
| No screenshot baselines made or compared | `@visual` compares only inside the Playwright Docker image (`bun run test:visual`); no Linux baselines exist for these components yet | To do before merge with Docker |

## Test files

- `site/tests/component/page-lists.tester.test.tsx` (tester): schemas, Markdown, missing data, edge cases, index budget, server rendering
- `site/tests/e2e/components/page-lists.tester.spec.ts` (tester): every fixture page on every screen (clipping, 200% text and zoom, motion, forced colours, print), plus the component's own checks named above
- `site/tests/component/page-lists.test.tsx`, `site/tests/e2e/components/page-lists.spec.ts` (builder)
- `site/tests/integration/budgets.test.ts`: the component index in the build within ADR 0001's budget

## Sign-off

☑ Every area and every screen above is passed or has an exception listed above; the open questions are Karthik's to answer and do not block merge.
Signed: Opus 5.5 component tester, 21 November 2026
