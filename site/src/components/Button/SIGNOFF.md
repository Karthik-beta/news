# Button — test sign-off

Status: **signed off**

Tester: Opus 5.5 component tester (not the builder) · Builder: Opus 5.5 component builder (issue #11, part 2) · Date: 22 November 2026
Branch / commit: `phase-3/components/controls`, components at 8ad90e1; tests, fixes and this sign-off in the tester's commits on top of it

## All areas

| Area | Result | Tests | Notes |
|---|---|---|---|
| Rendering | ☑ pass | `controls.tester.test.tsx` Button; `controls.test.tsx` / `controls.spec.ts` | Default secondary `type=button` (never submits by accident); primary, text, submit, extra classes and native attributes pass through |
| Data | ☑ n/a | — | Holds no data |
| Content edge cases | ☑ pass | `controls.tester.spec.ts` gallery "every case reads" | A long label wraps inside the button on phones |
| Formatting | ☑ n/a | — | Formats nothing |
| Server rendering | ☑ pass | `tests/ssr/primitives.test.tsx` (every case in Bun); `controls.tester.test.tsx` "server rendering" | Complete HTML, no script, no inline style; hydrates with no warnings (busy and idle) |
| Design-system compliance | ☑ pass | `controls.tester.spec.ts` "buttons: primary, secondary, text and disabled drawn from the tokens, in both themes" | Fira 500 15px, 10px 18px, 5px radius; primary ink/paper, secondary paper with the control border, text underlined ink-2; hover mixes paper into the primary fill |
| Accessibility | ☑ pass | `controls.tester.spec.ts` gallery axe (system + `data-theme`, forced colours); `controls.test.tsx` / `controls.spec.ts` | Disabled is `disabled` and described by its reason. Screen readers checked: none by hand |
| Interaction | ☑ pass | `controls.tester.spec.ts` "buttons: Tab reaches each enabled button with the focus ring; the disabled one is skipped" | 2px focus ring 2px out on every variant; disabled is not focusable and does not react to hover |
| Themes (light, dark ×2, forced colours, print) | ☑ pass | `controls.tester.spec.ts` gallery axe by system and `data-theme`; token tests in both themes; forced colours; `@print` test | Light, dark by system setting and by `data-theme`; forced colours axe-clean after BT-1 |
| Motion | ☑ pass | `controls.tester.spec.ts` gallery "nothing animates" (reduced and @motion) | No animation; no transition over 200 ms |
| Zoom (200% zoom, 200% text) | ☑ pass | `controls.tester.spec.ts` gallery "reflows at 200% zoom and at 200% text size" | No sideways scroll or clipped text; Firefox and WebKit included |
| Failures (images, video, data) | ☑ pass | `controls.tester.spec.ts` gallery "still reads with the web fonts blocked" | No images, video or fetched data; fonts blocked still reads |
| Performance (JS budget, CLS, render time) | ☑ pass | `controls.tester.spec.ts` gallery "ships no JavaScript and does not move"; `tests/ssr/primitives.test.tsx` render time | JS: 0 KB. Layout shift after fonts < 0.001 (Chromium). Render p95 under 20 ms in Bun |
| Cross-browser (Chromium, Firefox, WebKit) | ☑ pass | both controls specs in `chromium-*`, `firefox-*`, `webkit-*` | Firefox and WebKit at 360 and 1440 with their 200% runs (light scope). WebKit on Windows does not Tab to buttons or links (Safari's default): those tests focus directly |

## Different screens

| Width | Light | Dark | Notes |
|---|---|---|---|
| 320 | ☑ | ☑ |  |
| 360 | ☑ | ☑ | also Firefox and WebKit |
| 390 | ☑ | ☑ |  |
| 430 | ☑ | ☑ |  |
| 600 | ☑ | ☑ |  |
| 768 | ☑ | ☑ |  |
| 980 | ☑ | ☑ |  |
| 1120 | ☑ | ☑ |  |
| 1440 | ☑ | ☑ | also Firefox and WebKit |
| 1920 | ☑ | ☑ |  |
| Landscape phone 844×390 | ☑ | ☑ |  |
| Touch (phone, tablet) | ☑ | ☑ | Touch emulated below 980px: every target ≥ 24 × 24px |
| Scale factors 1× / 2× / 3× | ☑ | ☑ | 3× phones, 2× tablets and small laptops, 1× desktops |
| In context (article, home fixtures) | ☑ | ☑ | Alone and inside the page shell's gallery only: the search, 404 and report-an-error page templates are #14, so there is no page to test them in yet |

Runs (all ten controls together):

Scope: Karthik's light test scope: every width in Chromium; Firefox and WebKit at 360 and 1440 with their 200% text
and zoom runs; Vitest for the rest; `test:integration` once; one worker throughout. No Docker baselines (`@visual`
skips outside the Playwright image).

Each Chromium project ran one theme by the system setting; every axe and colour test also switches to the other theme
by `data-theme` inside the test, so both themes are checked at every width.

- **Chromium, first slice** (`chromium-light-360`, `chromium-dark-1440`, both specs): first pass 76 runs, 12 failed:
  two defects (RF-1, NF-1) and tester test bugs (a helper used inside the page, a heading locator, the fixture POST
  aborting, the clipped-text helper counting a long search query that scrolls inside its input); fixed. Re-run: 206
  runs, 175 passed, 31 skipped, 0 failed.
- **Chromium matrix** (light 320, 430, 768, 1120, landscape; dark 390, 600, 980, 1920, landscape; print; motion;
  forced colours light and dark): 1,063 runs, 912 passed, 142 skipped (by design: `@visual` outside Docker, 200% zoom
  below 640px, touch tests on non-touch screens, WebKit-only branches), 9 failed: RF-2 (1) and BT-1 (8), fixed.
  Re-run of the failing projects: 38 runs, 34 passed, 4 skipped; report form and button tests on smoke, 320, 390 and
  landscape: 85 runs, 75 passed, 10 skipped, 0 failed.
- **Firefox and WebKit** (`firefox-light-360`, `firefox-dark-1440`, `webkit-light-360`, `webkit-dark-1440`): 412 runs,
  348 passed, 64 skipped, 0 failed.
- **Vitest** (unit, component, astro; `--maxWorkers=1 --pool=forks`): 1,581 tests in 41 files, 1,581 passed, 0 failed (before NF-2 and IV-1); the affected files re-run after them (config pin, both controls files, primitives): 257 passed.
- **SSR in Bun** (`bun test tests/ssr`, controls added to the primitives galleries): 172 passed, 0 failed.
- **Integration** (`bun run test:integration` in `site/`: HTML validity, links, budgets; `--maxWorkers=1`): first run 494 tests, 491 passed, 3 failed (NF-2, IV-1); fixed, re-run: 494 passed.
  The component index is within ADR 0001's budget (554 of 4,000 tokens, 25 entries); none of the controls is in it,
  as primitives.
- **After NF-2 and IV-1**: the 404, text field and report form tests on Chromium 360 and 1440, Firefox 360, WebKit
  1440, forced colours and print: 120 runs, 101 passed, 19 skipped, 0 failed.
- Token lint clean; Biome clean on the changed files.
- No failure appeared only under load; nothing was re-run to make it pass. SF-1 is held by `test.fail` so the run
  goes red when it is fixed and the marker must come off.

Screen readers: not checked by hand (no NVDA, VoiceOver or TalkBack on this machine). Roles, names, descriptions and
landmarks are asserted instead, and axe runs in both themes on every screen run.

## Issues found

| # | Area / screen | Problem | Fix (commit) |
|---|---|---|---|
| BT-1 | Forced colours, Chromium, light and dark | axe colour-contrast (serious) on the primary button: Chromium forces `color` but leaves `-webkit-text-fill-color` at the paper value, which axe reads as paper on ButtonFace (the button itself painted correctly). Failed the forced-colours gallery test for Button, SearchField, NotFound and ReportForm | `color: ButtonText` on `.button--primary` under `forced-colors: active` (268e610) |

## Accepted exceptions

| Exception | Reason | Approved by |
|---|---|---|
| Disabled at 40% opacity reads faint by eye, most in dark | Checked by eye at 360 in both themes: the words stay legible; WCAG 1.4.3 exempts disabled controls and the reason beside it is at full contrast | Tester |
| Buttons are not printed | Base print CSS drops interface buttons; nothing to press on paper | Tester |

## Sign-off

☑ Every area and every screen above is passed or has an accepted exception.
Signed: Opus 5.5 component tester, 22 November 2026
