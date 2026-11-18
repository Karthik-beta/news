---
name: news-design-system
description: "How to apply the News design system (Direction E, \"Economist school\") to any UI work in this repo. Use this whenever you write or change CSS, components, charts, page templates, Board (the egui desktop app), social images, colours, fonts, spacing or any visual element, and whenever you change a design token or a design document, even if the request doesn't mention \"design system\"."
---

# News design system

The design is agreed and documented. Your job is to apply it exactly, not to reinterpret it. Everything visual on the site must trace back to a token or a rule in `design/`.

## Read first (only what the task needs)

| Task | Read |
|---|---|
| Any visual work | `design/principles.md` (short; the rules that settle disputes) |
| Colours, chart colours | `design/color.md` |
| Fonts, sizes, numerals | `design/typography.md` |
| Page structure, spacing, breakpoints | `design/layout.md` |
| Any chart or data table | `design/charts.md` |
| A component | its section in `design/components.md` |
| Words on the page (labels, errors, numbers, dates) | `design/content-style.md` |
| Accessibility | `design/accessibility.md` |
| Exact values | `design/tokens/tokens.json` (source of truth) |

The mock-ups in `design/mockups/` and `design/brand-board.html` illustrate the look. When they differ from the documents, the documents and `tokens.json` win.

## Rules that are easy to get wrong

- **Tokens only.** Use the CSS custom properties from `tokens.css` (`--color-ink`, `--chart-highlight`, `--font-family-serif`, …). Never write a hex colour, a font family name or a px font size in a component. The token lint and the reviewers will reject it.
- **Crimson is brand and emphasis, never "down".** `--color-brand` and `--chart-highlight` mark the wordmark, fly titles, the chart rule and tag, the one highlighted data item, current-nav and pagination underlines, and form errors. Rises and falls are written in words or signed figures in ink colours. Never red/green for down/up.
- **One highlight per chart.** Everything else is `--chart-context`, `--chart-comparison` (one series at most) or `--chart-neutral`. The categorical palette is only for charts where every series matters equally, used in slot order, never cycled.
- **No boxes around content.** Structure comes from rules (hairline, ink, 2px section, brand rule + 36×7px tag) and spacing. Radius 0 on content; 3px on badges, 5px on controls. No shadows, gradients or decorative images.
- **Two typefaces, fixed jobs.** Literata for the wordmark, headlines and reading text; Fira Sans for fly titles, chart text, figures, tables and interface. No third face, no monospace, no all-caps labels.
- **Numbers as the source states them.** Original currency, units and fiscal period labels; true minus sign (−); formats in `content-style.md`.
- **Both themes.** Every colour comes from a token with a dark value; check your work in light and dark. Dark mode is not an inversion.
- **Motion only in response to the reader**, ≤ 200 ms, off under `prefers-reduced-motion`.

## Changing a token

Tokens are shared by thousands of pages, so a change is deliberate:

1. Edit `design/tokens/tokens.json` (light value in `$value`, dark value in `$extensions["news.dark"]`).
2. If it's a **chart colour**, re-validate both themes with the dataviz skill's `validate_palette.js`:
   `node <dataviz>/scripts/validate_palette.js "<slots>" --mode light --surface "#F4F4F1"` and `--mode dark --surface "#17181A"`; add `--pairs all` for the first three categorical slots. All hard checks must pass.
3. If it's a **text colour**, check WCAG contrast ≥ 4.5:1 on both `paper` and `wash`, in both themes.
4. Regenerate the CSS: `bun design/tokens/build.mjs` (or `node`). Never edit `tokens.css` by hand.
5. Update the tables in `design/color.md` (or the relevant document) and add a line to the decisions log in `design/README.md`.
6. Brand-level changes (crimson, paper, ink, fonts) need Karthik's approval before merge.

## When the design docs don't cover something

Decide from `principles.md`, keep it consistent with the nearest existing component, and write the decision into the relevant design document in the same PR, so the next agent finds it.
