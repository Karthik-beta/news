# News — Design system

Nov 14, 2026 · Agreed with Karthik · Build plan step 3 ("Design direction") complete

The design is **Direction E, "Economist school"**: near-white paper, black ink, a single crimson for brand and emphasis, Literata for journalism and Fira Sans for data, and a house chart style with a crimson rule and tag across the top of every chart.

It was chosen over five other directions after two rounds of side-by-side specimens. The full mock-up (a curated home page and an Apple fiscal 2024 earnings page, light and dark) is in [mockups/](mockups/).

## The design in one screen

| | |
|---|---|
| Personality | A meticulous analyst: precise, dry, confident; the numbers speak |
| References | The Economist and the FT: conventions borrowed, brand assets not |
| Paper | `#F4F4F1` near-white (dark: `#17181A`) |
| Ink | `#181818` (dark: `#EDEDEA`) |
| Brand | Crimson `#C8213A` (dark: `#F4687C`); brand and emphasis only, never "down" |
| Data | Blue `#065DA0` for comparison, pale blue-grey for context, an 8-colour validated categorical palette |
| Type | Literata (headlines, reading text); Fira Sans (charts, figures, labels, interface) |
| Signature | The crimson rule and tag on every chart; the crimson fly title above every headline |
| Density | Balanced: dashboard sections with room to breathe |
| Motion | None, except brief responses to the reader |
| Theme | Light by default; dark follows the device |
| Accessibility | WCAG 2.2 AA; colour-blind-validated chart colours |

## Documents

| Document | What it decides |
|---|---|
| [principles.md](principles.md) | The rules that settle every other question, and what we never do |
| [color.md](color.md) | Interface colours, contrast, chart colour roles, categorical, sequential and diverging palettes |
| [typography.md](typography.md) | Fonts, loading, type scale, numerals, line length |
| [layout.md](layout.md) | Breakpoints, grid, spacing, rules, page templates (article, home, hubs, search, static) |
| [charts.md](charts.md) | The house chart style: anatomy, choosing a form, marks, axes, labels, interaction |
| [components.md](components.md) | Visual spec for every component family the library needs |
| [content-style.md](content-style.md) | Headlines, fly titles, chart titles, numbers, units, dates, interface text |
| [accessibility.md](accessibility.md) | WCAG 2.2 AA rules as they apply to this site, and testing |
| [../skills/news-brand/SKILL.md](../skills/news-brand/SKILL.md) | The short rule set for the page-writing agents (cached prompt prefix) |
| [tokens/tokens.json](tokens/tokens.json) | **Source of truth** for every token (W3C design tokens format; dark values in `$extensions["news.dark"]`) |
| [tokens/tokens.css](tokens/tokens.css) | CSS custom properties generated from `tokens.json`, light and dark |
| [tokens/build.mjs](tokens/build.mjs) | Regenerates `tokens.css`: `bun design/tokens/build.mjs` or `node design/tokens/build.mjs` |
| [brand-board.html](brand-board.html) | One-page visual summary: colours, type, signatures and rules, light and dark |
| [mockups/mockup-direction-e.html](mockups/mockup-direction-e.html) | Reference mock-up: home page and article page |
| [mockups/design-directions.html](mockups/design-directions.html) | The six directions that were compared (record of the decision) |

When documents disagree, `tokens.json` wins for values and `principles.md` wins for rules. The mock-ups illustrate; they are not the spec.

## How the pieces fit the build plan

1. **Component library (Opus 5.5).** Import `tokens.css`; implement each family in [components.md](components.md) and the chart types in [charts.md](charts.md). Every component reads tokens only.
2. **Brand skill.** [skills/news-brand](../skills/news-brand/SKILL.md) is the skill the Sonnet page writers load. Keep it under about 1,000 words; it lives in the cached prefix.
3. **Quality checks.** Add to `news check`: axe-core with no serious or critical violations in both themes; every chart has a title, subtitle, source line and data table; no literal colours in rendered pages; crimson not used for decreases.
4. **Astro shell.** Self-host Literata and Fira Sans (see [typography.md](typography.md#loading-the-fonts)); build the masthead, footer, home, hub, entity, search and static templates from [layout.md](layout.md).

## Decisions log

| Date | Decision |
|---|---|
| 14 Nov 2026 | Interview: FT/Economist taste, meticulous-analyst personality, balanced density, light default + dark mode, takeaway chart titles, quiet citations, placeholder wordmark, "the look" as the memorable thing |
| 14 Nov 2026 | Round 1 directions (Ledger, Bulletin, Claret) judged too far from FT/Economist |
| 14 Nov 2026 | Round 2: Direction E (Economist school) chosen |
| 14 Nov 2026 | Near-white paper kept (overrides the earlier "tinted paper" preference); crimson as brand only, never "down" |
| 14 Nov 2026 | Crimson lightened from `#B8152F` to `#C8213A` |
| 14 Nov 2026 | Chart palettes generated and validated by script for colour-vision deficiency and contrast, both themes |

## Open items

- **Name and wordmark.** "News" in Literata 800 is a placeholder. A real name brings a real wordmark and favicon.
- **Favicon and social card.** Until the wordmark exists: a crimson square with a paper "N" in Literata 800, and the chart social image spec in [charts.md](charts.md#social-images).
- **Component-level validation.** Each component is checked at 360px, in both themes, with a keyboard and a screen reader as it's built.
- **Real-content test.** Before launch, render one page of each type (earnings, macro, report, policy, news, science, explainer) with real data and review them together.
- **Map details.** Projections per region and the country-name style for maps are settled when the map component is built.
