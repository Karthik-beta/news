# ReportForm — test sign-off

Status: **signed off**

Tester: Opus 5.5 component tester (not the builder) · Builder: Opus 5.5 component builder (issue #11, part 2) · Date: 22 November 2026
Branch / commit: `phase-3/components/controls`, components at 8ad90e1; tests, fixes and this sign-off in the tester's commits on top of it

## All areas

| Area | Result | Tests | Notes |
|---|---|---|---|
| Rendering | ☑ pass | `controls.tester.test.tsx` "report-an-error form"; `controls.test.tsx` / `controls.spec.ts` | Blank form (Vitest only, axe-clean), errors, sent; `ErrorSummary` alone; unknown error keys ignored; intake errors on `where` listed in field order |
| Data | ☑ pass | `controls.tester.test.tsx` "checks" | `reportErrors`: nothing given, whitespace-only (tabs, NBSP), email shapes accepted and refused, spaces trimmed, 20,000-character report |
| Content edge cases | ☑ pass | `controls.tester.spec.ts` gallery at 200% text on 320 | A long error message breaks inside the summary (RF-2) |
| Formatting | ☑ n/a | — |  |
| Server rendering | ☑ pass | `tests/ssr/primitives.test.tsx` (every case in Bun); `controls.tester.test.tsx` "server rendering" | Complete HTML, no script, no inline style; hydrates with no warnings (busy and idle) |
| Design-system compliance | ☑ pass | `controls.tester.spec.ts` "report form: the summary opens with a 2px error rule and no box …" | 2px error rule, no box, heading in ink with the error icon, links in the error colour, both themes |
| Accessibility | ☑ pass | `controls.tester.spec.ts` gallery axe; "controls galleries pass axe with the fields focused"; `controls.test.tsx` / `controls.spec.ts` keyboard order | Summary `aria-live=polite`; keyboard path summary → fields → Send report (Chromium, Firefox). Screen readers checked: none by hand |
| Interaction | ☑ pass | `controls.tester.spec.ts` "each summary link focuses its field with no JavaScript …", "sending posts the form with the page" | All three engines focus the field when a summary link is followed (HTML's scroll-to-fragment runs the focusing steps), so no script is needed; the label stays in view (RF-1); the POST carries page, what, where and email |
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
| RF-1 | see TextField | Following a summary link hid the field's label | Fixed in the shared shell (8e2927b) |
| RF-2 | Chromium 320, 200% text | The summary link "…name@example.com, or leave it empty." could not break: the page was 332px wide | `overflow-wrap: anywhere` on `.error-summary__link` (268e610); the gallery's 200% text test |
| BT-1 | Forced colours | Send report button failed axe (see Button) | 268e610 |
| IV-1 | see TextField | `noValidate` and `autoComplete` in camelCase failed html-validate | Validator config (b117717) |

## Accepted exceptions

| Exception | Reason | Approved by |
|---|---|---|
| Send report is not printed | Base print CSS drops buttons; a printed form cannot be sent. Fields, summary and sent message print | Tester |
| The summary is not announced or focused when the server re-renders the page with errors | A live region does not speak on page load and the form has no script. The page template (issue #14) should start the title with "Error:"; whether to autofocus the summary is a question for Karthik | Open for #14 |

## Sign-off

☑ Every area and every screen above is passed or has an accepted exception.
Signed: Opus 5.5 component tester, 22 November 2026
