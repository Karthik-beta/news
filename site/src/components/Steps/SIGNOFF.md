# Steps — test sign-off

Status: **signed off, with an open question for Karthik (below)**

Tester: Opus 5.5 component tester (not the builder) · Builder: Opus 5.5 component builder (issue #11, extension set) · Date: 28 November 2026
Branch / commit: `phase-4/site/extension-set` (PR #107), component at 4e25bd6; tests, the in-context fixture and this sign-off in the tester's commits on top of it (with origin/main merged in)

Scope: every width in Chromium, light and dark; Firefox and WebKit at every width with one theme by system setting per
width (alternating), plus landscape, print and motion; forced colours in Chromium (the only engine that emulates it).
Every axe and token test also switches to the other theme by `data-theme` inside the test, so both themes are checked at
every width in every engine. One worker throughout.

## All areas

| Area | Result | Tests | Notes |
|---|---|---|---|
| Rendering | ☑ pass | `tests/unit/extension-set.tester.test.ts` "Steps"; `tests/component/extension-set.test.tsx` (builder) | 2, 3, 5 and 8 steps numbered 1…n by code; numbers `aria-hidden` (the list gives the order); title plain text (a `{ref}` in a title resolves, never a popover); one source line under the list |
| Data | ☑ pass | `extension-set.tester.test.ts` "every reference kind resolves", "an unknown value says No data", "one source line … each source once" | dp, pd, q, cx and dv references; an unknown value says "no data" and is reported; each cited source once in the source line however many steps cite it; no source line when nothing is cited (periods, glossary links) |
| Content edge cases | ☑ pass | `extension-set.tester.spec.ts` "nothing clipped, outside the screen, overlapping or under 12px" on every screen; `edge-cases` and `in-context` fixtures | Two steps, eight steps, a title that wraps, a 45-letter word, a 400-character quote, duplicate titles |
| Formatting | ☑ pass | `extension-set.tester.test.ts` "currencies, scales, separators and true minus: exactly as Prose writes them" | $, €, £, ¥, ₹, ₩, NT$, HK$, A$, CHF; m/bn/trn; 167,045; true minus; ≥ 1e12 and 1.4e-8; null. Identical to Prose for the same sentence; periods as the company labels them ("fiscal 2024") |
| Server rendering | ☑ pass | `extension-set.tester.spec.ts` "JavaScript only for chart islands"; `extension-set.spec.ts` "ship no JavaScript" (builder) | Complete static HTML; no script requested on the Steps pages; 0 KB JS |
| Design-system compliance | ☑ pass | `extension-set.tester.spec.ts` "Steps › in the tokens of this theme", "steps are 24px apart" | Number Fira 600 headline-s, ink; title Fira 600 `type.ui`; text Literata `body-s`; rule 1px `color.rule`; no background, radius or shadow on any part, in both themes by system setting and by `data-theme`; 24px between steps, 16px to the source line |
| Accessibility | ☑ pass | axe in both themes on every screen (`extension-set.tester.spec.ts`, builder's spec); "the accessibility tree: one list of steps"; forced colours | axe: zero violations of any impact, light and dark. Accessibility tree asserted: one list, one list item per step with its title and text, the source line after it. Screen readers checked by hand: none (no NVDA, VoiceOver or TalkBack on this machine); see Q-1 |
| Interaction | ☑ pass | "every value in a step opens its source popover by keyboard" | Enter opens a value's source popover, Escape closes it and focus returns; nothing else is interactive |
| Themes (light, dark ×2, forced colours, print) | ☑ pass | token, forced-colours and print tests | Forced colours: the rule stays visible (not Canvas, not transparent), axe clean; print: every step, its rule and the source line print, nothing clipped |
| Motion | ☑ pass | "nothing animates under reduced motion"; "nothing animates on load; transitions within 200 ms" (@motion) | No animation; 0 ms under reduced motion |
| Zoom (200% zoom, 200% text) | ☑ pass | "reflows at 200% text and 200% zoom"; "the rule still joins the numbers at 200% text" | The rule starts under its number's glyph, ends above the next number, runs down the number's centre (≤ 1.5px) and stays left of the text; the title stays on the number's row, at 100% and 200% text |
| Failures (images, video, data) | ☑ pass | "still reads with the web fonts blocked"; unknown-value tests | No images or video; unknown values say "no data"; fallback fonts: nothing clipped or off screen |
| Performance (JS budget, CLS, render time) | ☑ pass | "JavaScript only for chart islands; no layout shift once fonts are in" | JS: 0 KB. Layout shift after fonts < 0.001 (Chromium) |
| Cross-browser (Chromium, Firefox, WebKit) | ☑ pass | both specs in `chromium-*`, `firefox-*`, `webkit-*` | See Runs |

## Different screens

| Width | Light | Dark | Notes |
|---|---|---|---|
| 320 | ☑ | ☑ | |
| 360 | ☑ | ☑ | |
| 390 | ☑ | ☑ | |
| 430 | ☑ | ☑ | |
| 600 | ☑ | ☑ | |
| 768 | ☑ | ☑ | |
| 980 | ☑ | ☑ | |
| 1120 | ☑ | ☑ | |
| 1440 | ☑ | ☑ | |
| 1920 | ☑ | ☑ | |
| Landscape phone 844×390 | ☑ | ☑ | |
| Touch (phone, tablet) | ☑ | ☑ | Touch emulated below 980px; targets ≥ 24px |
| Scale factors 1× / 2× / 3× | ☑ | ☑ | 3× phones, 2× tablets and small laptops, 1× desktops |
| In context (article, home fixtures) | ☑ | ☑ | New fixture `Steps/in-context`: an explainer body (prose, a source card, a heading, the steps, a chart with notes, prose) in the article page shell: blocks in order ≥ 16px apart, left edges aligned with the prose, axe-clean in both themes on every screen. No home fixture uses it (steps belong in explainers) |

Runs (laptop shared with another agent's Rust build, one Playwright worker; `extension-set.tester.spec.ts` and the
builder's `extension-set.spec.ts` together, all six extension-set fixture pages each time):

- **Chromium, first slice** (`chromium-light-360`): 69 runs, 10 failed: 6 tester test bugs (a token probe without a
  border style, the sources panel and the chart source line looked up by the wrong class, the card's link counted with
  the panel's, popover panels counted as boxes in the steps) fixed and re-run, and 4 LC-1 (LineChart notes).
- **Chromium matrix**, every width light and dark, both landscapes, print, motion, forced colours light and dark:
  1,542 runs, 1,270 passed, 212 skipped (by design: `@visual` outside Docker, 200% zoom below 640px, keyboard tests
  on touch phones), 60 failed, all LC-1. Nothing failed on a SourceCard or Steps page.
- **Firefox** (light 320, 390, 600, 980, 1440; dark 360, 430, 768, 1120, 1920; both landscapes; print; motion): 840
  runs, 708 passed, 97 skipped, 35 failed: 33 LC-1 and 2 timeouts under load (axe on `Steps/in-context` at 600 and the
  builder's axe test on `Steps/edge-cases` at 980, 1.1 and 1.9 minutes); both re-run alone: passed.
- **WebKit** (the same projects): 840 runs, 675 passed, 133 skipped (also: WebKit does not tab to links by default),
  32 failed, all LC-1.
- **Vitest** (`--maxWorkers=1`): 2,204 tests in 61 files (unit, component, astro), 2,203 passed, 1 failed: SC-1, a SourceCard Markdown defect (the SourceCard test file, `tests/unit/extension-set.tester.test.ts`). Run with a local placeholder `SourceCard/SIGNOFF.md` so the folder-contract checks could run; it is not committed.
- **SSR in Bun** (`bun test tests/ssr`): 184 passed, 0 failed. `bun run typecheck`: 0 errors; `bun run lint`: no errors (5 older warnings elsewhere); `bun run validate`: OK.

## Issues found

None in Steps.

## Accepted exceptions and open questions

| Exception | Reason | Approved by |
|---|---|---|
| Q-1: Safari drops list semantics from an `<ol>` with `list-style: none` unless it has `role="list"`, and the step numbers are `aria-hidden`, so VoiceOver on Safari would read the steps in order but without "list, 4 items" and without numbers | Not checkable here (no VoiceOver). Every house list (Timeline, the LineChart notes, the sources list) has the same pattern; order is kept, so WCAG 1.3.1 and 1.3.2 are met. One-line fix if wanted: `role="list"` on `.steps__list`, or drop `aria-hidden` from the number | Pending Karthik |

## Screenshots

Baselines made in the Playwright Docker image (`bun scripts/visual.ts … --update-snapshots=all`, site/TESTING.md,
"Screenshots") on the screens #71 chose: Chromium light and dark at 360 and 1440, light at 768 and 980, for
`Steps/default`, `Steps/edge-cases`, `Steps/in-context`, `SourceCard/default` and `SourceCard/edge-cases` (30 PNGs in
`site/tests/e2e/__screenshots__/<project>/components/extension-set.tester.spec.ts/`, 2.9 MB). Reason for accepting
them: first baselines of new components; each image reviewed against the Windows review screenshots of the same
screens (same layout, wrapping and colours; only the font rasterisation differs). Tagged `@smoke`, so CI compares the
smoke screens on every PR. `LineChart/notes` has no baseline while LC-1 is open; `Steps/in-context` holds a chart with
three well-spaced notes, so the LC-1 fix may change that image: re-make it then.

## Test files

- `site/tests/unit/extension-set.tester.test.ts` (tester): the "Steps" describe
- `site/tests/e2e/components/extension-set.tester.spec.ts` (tester): every fixture page on every screen, tokens, the rule's geometry, the accessibility tree, keyboard, in context, screenshots
- `site/tests/component/extension-set.test.tsx`, `site/tests/e2e/components/extension-set.spec.ts` (builder)
- Fixture added by the tester: `Steps/fixtures/in-context.oui` and its Markdown snapshot

## Sign-off

☑ Every area and every screen above is passed or has an open question listed.
Signed (pass): Opus 5.5 component tester, 28 November 2026
