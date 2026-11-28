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
| [mockups/mockup-direction-e.html](mockups/mockup-direction-e.html) | Reference mock-up: home page and article page (the home mock-up predates the "Latest" list) |
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
| 14 Nov 2026 | Home page becomes dynamic and agent-curated every 2–3 days (server-side data, no rebuild); an automatic "Latest" list added under the lead block (not yet in the mock-up) |
| 16 Nov 2026 | Figures show the source's own digits: rates, points and basis points are never rounded (6.50% stays 6.50%) |
| 16 Nov 2026 | Citation and glossary popovers open on click, tap, Enter or Space only, not on hover or focus |
| 16 Nov 2026 | Crimson never marks a subject that fell (chart highlight rules follow with the chart components, issue #8) |
| 20 Nov 2026 | New token `chart.neutral-highlight` (`#3D3D3A` light, `#B0B0AB` dark) for a highlighted subject that fell; what "fell" means per chart is in [charts.md](charts.md#when-the-subject-fell) (issue #8) |
| 21 Nov 2026 | Charts II (issue #13): waterfall rows say what they do (total, deduct, add) with values as published; the Sankey is built from the waterfall's rows and shows that waterfall on narrow plots and when the rows cannot flow; small Sankey flows fold into an "Other" that writes no number; "fell" rules for waterfalls, Sankeys, dot plots, slopes, stacked and segmented bars ([charts.md](charts.md#when-the-subject-fell)); dot plots and slopes have no axis, every end carries its value |
| 22 Nov 2026 | Charts III part 1 (issue #13): small multiples reuse the line and column plots with one shared x and value axis; histograms bin on round numbers and write no count on the plot (a count is ours); heatmaps shade on five sequential blue steps, 300–700 in light mode and 600, 400, 300, 200, 100 in dark (dark 500 has no text colour that reads on it), with values in ink or the paper colour at 4.5:1 or more ([charts.md](charts.md#heatmaps)) |
| 22 Nov 2026 | Controls and states (issue #11): new size tokens `font.size.control` (16px, form fields) and `font.size.search` (18px); form labels use `type.ui` (15px) and hints `type.label`; required and optional say so in the label; the tooltip stays part of `Popover`; empty and error states share one look (`StateMessage`); loading shows no spinner at all ([components.md](components.md#controls)) |
| 28 Nov 2026 | Extension set (issue #11, approved by the orchestrator for Karthik): `SourceCard`, `Steps` and numbered notes on `LineChart` ([components.md](components.md#steps-steps), [charts.md](charts.md#line-charts-and-step-lines)); no `@openuidev/react-ui` base component is given to the page writer (reasons in `site/src/components/EXTENSIONS.md`) |
| 28 Nov 2026 | Social images (issue #71): a chart too tall for the 1200×675 card (every Sankey, long waterfalls, bars, dot plots and slopes) gets a 1200×1200 square by the same rules instead of being refused; one too tall for the square is still an error ([charts.md](charts.md#social-images)) |
| 18 Nov 2026 | Board (the egui desktop app) shares only the tokens with the site: `cli/crates/news-board/build.rs` generates Rust tables (both palettes, type sizes, spacing, radii, borders) from `tokens.json`, and a test fails on literal colours in Board code. It follows the Windows light or dark setting with a manual override. Board uses egui's bundled face until Literata and Fira Sans files are added to it; the sizes are the tokens'. Section rules (2px ink) and the crimson current-nav underline carry over; radius 0 on content, 5px on controls |
| 28 Nov 2026 | Board labelling (issues #19, #71): a label is an answer, not a verdict, so its buttons carry no colour (Pass and Fail, Same story and Different are neutral controls; crimson is never good or bad, up or down). One item on screen at a time, a button or a key per label (P or F, S or D, K to skip), and nothing on screen that would anchor it: no judge verdict, seeded-fault kind or cosine. Huddle's form is rules and spacing under the run it belongs to, with a hash shown the way a gate's is |

## Open items

- **Name and wordmark.** "News" in Literata 800 is a placeholder. A real name brings a real wordmark and favicon.
- **Favicon and social card.** Until the wordmark exists: a crimson square with a paper "N" in Literata 800, and the chart social image spec in [charts.md](charts.md#social-images).
- **Component-level validation.** Each component is checked at 360px, in both themes, with a keyboard and a screen reader as it's built.
- **Real-content test.** Before launch, render one page of each type (earnings, macro, report, policy, news, science, explainer) with real data and review them together.
- **Map details (proposal, awaiting Karthik's approval).** Built with the map component (issue #13), to be confirmed:
  - *Projections:* Equal Earth for the world (central meridian 0, Antarctica left out), the only map today. A national map (US states in Albers equal-area with Alaska and Hawaii inset; other countries in their own equal-area or conformal projection) is added with the first page that needs one, its geometry from Natural Earth admin-1 projected by the same build script (`site/scripts/gen-map-geometry.ts`).
  - *Country names:* the page writer gives each country's short common name ("United States", "South Korea", "Czechia", "Democratic Republic of the Congo"), not the formal name or Natural Earth's abbreviations ("Dem. Rep. Congo"); the geometry carries only ISO 3166-1 alpha-3 codes (Natural Earth's own code where there is none: Kosovo `KOS`).
  - *Classes:* up to five, not the "5–7 steps" [charts.md](charts.md#maps) first said: only five steps of the sequential blue ramp pass the validator on paper in light mode (300–700; 100 and 200 are below the 2:1 floor), so the map uses the heatmap's validated steps. Breaks are round numbers (one significant figure) near the quintiles, so each shade holds about as many countries; equal ranges put most countries of a skewed measure in one shade.
