---
name: news-component-testing
description: "The full test procedure for a News component or chart: every area (rendering, data edge cases, formatting, server rendering, design-system compliance, accessibility, interaction, themes, motion, zoom, failures, performance, cross-browser) and every screen size, ending in a written sign-off. Use this whenever you test, verify, QA, screenshot-test or sign off a component, chart, primitive or fixture page. Opus 5.5 agents only, and never the agent that built the component."
---

# Testing a News component

Every component should be well tested in all areas and also be well tested with different screens (Opus 5.5 agent only).

That sentence is the project rule (Development Plan, Phase 3.4). Component testing is done by an Opus 5.5 tester that did not build the component, because a fresh reader catches what the builder stopped seeing. Sonnet agents never write or run component tests. A component is not done until its `TESTING.md` sign-off exists.

Load `news-design-system` first, and read the component's spec in `design/components.md` or `design/charts.md` and its `spec.md`.

## 1. All areas

Work through every row. Write tests for each; don't just look.

| Area | Must cover | Tooling |
|---|---|---|
| Rendering | Every prop, variant and default | Vitest + Testing Library |
| Data | References resolve; every figure carries a source ID; missing, null, zero, negative, very large (≥ 1e12) and very small numbers; single-point and long series; maximum items | Unit tests with edge fixtures |
| Content edge cases | Very long and very short headlines/labels, long unbroken words and URLs, many categories, empty lists, one item | Fixtures + visual tests |
| Formatting | $, €, £, ¥, ₹, ₩, NT$, HK$, A$; bn/m/trn; true minus; thousands separators; dates; fiscal periods exactly per `design/content-style.md` | Unit tests |
| Server rendering | Static HTML complete without JS; hydration matches with no warnings; static components ship 0 KB JS | SSR tests in Bun |
| Design-system compliance | Tokens only; crimson never encodes a decrease; one highlight; spacing on the scale; radius/shadow rules | Token lint + manual check against docs |
| Accessibility | axe-core: zero serious/critical; roles, names, heading order; keyboard (tab order, arrows in charts, Escape closes popovers); visible focus; chart accessible name + data table; never colour-only | axe + Playwright keyboard tests; NVDA/VoiceOver/TalkBack spot checks on representative components |
| Interaction | Tooltips/popovers on hover, focus and tap; data-table toggle; click-to-load video; gallery overlay; form validation | Playwright |
| Themes | Light; dark via system setting and via `data-theme`; forced colours; print | Playwright emulation + screenshots |
| Motion | Nothing animates on load; transitions ≤ 200 ms; none under reduced motion | Playwright emulation |
| Zoom | 200% zoom and 200% text size: no lost content, no sideways page scroll | Playwright |
| Failures | Image load failure → alt-text placeholder at same size; video placeholder offline; data fetch absent | Playwright with blocked routes |
| Performance | JS budget per component; CLS 0; render time within budget | Bundle analysis, Lighthouse CI on the fixture page |
| Cross-browser | Chromium, Firefox, WebKit | Playwright projects |

## 2. Different screens

Screenshot every fixture at every width, in light and dark, in all three engines:

`320, 360, 390, 430, 600, 768, 980, 1120, 1440, 1920` px wide.

Also:
- touch emulation at phone and tablet widths (targets ≥ 24px, tooltips on tap);
- landscape phone 844 × 390 for charts and video;
- device scale factor 1 and 2 (3 for phones) to check line crispness;
- the component **alone** and **in context** (inside the fixture article page and fixture home page) to catch spacing and rhythm problems.

At each size check: no clipped or overlapping text, no label collisions, no text under 12px, no sideways page scroll, layout matches the spec for that breakpoint, and the component still reads.

The matrix config to start from is in `references/playwright-matrix.md`. Full runs are CPU-heavy: run one at a time on the laptop, never in parallel with another heavy job. CI only runs the smoke subset (360 and 1440, light and dark, Chromium).

## 3. Baselines

- Fixed fonts (self-hosted), fixed dates in fixtures, animations disabled, stable viewport and scale, so screenshots are deterministic.
- A screenshot diff fails the run. Accept a new baseline only with a reason written in `TESTING.md`.

## 4. Defects

Send each defect back to the builder with: the area, the screen (width, theme, engine), what's wrong, the expected behaviour from the spec, and the failing test. After two failed rounds, hand it to the Opus bug fixer. Re-run the affected areas and screens after every fix.

## 5. Sign-off

Copy `assets/TESTING.md` into the component folder and fill it in: every area and every screen ticked or explained, issues found and fixed, accepted exceptions with reasons, test file locations, and your sign-off line. Return to the orchestrator: pass/fail, counts, open exceptions.
