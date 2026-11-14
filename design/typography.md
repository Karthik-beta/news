# Typography

Two families, each with a clear job. The serif carries the journalism (headlines and reading text); the sans carries the data (charts, numbers, labels, interface).

| Family | Role | Why |
|---|---|---|
| **Literata** (serif, variable) | Wordmark, headlines, standfirsts, body text, section headings, notes | A bookish, humanist serif drawn for long reading on screens, in the spirit of The Economist's Milo Serif. Its optical-size axis keeps headlines crisp and body text sturdy. |
| **Fira Sans** (sans-serif) | Fly titles, chart titles and labels, key figures, tables, navigation, metadata, buttons, source lines | A humanist sans with clear figures and proper tabular numerals, legible at small sizes, in the role The Economist gives Econ Sans. |

Both are free under the SIL Open Font License, which allows self-hosting and embedding.

No third family. No monospace anywhere, including for data labels or code-like strings in normal pages.

## Loading the fonts

- **Self-host** the font files as WOFF2 from the site's own R2 bucket. Do not load them from Google Fonts at runtime: the PRD promises no third-party tracking, and Google Fonts receives each visitor's IP address.
- Subset to Latin and Latin Extended (covers English, European names, and the currency symbols we use: $ € £ ¥ ₹ ₩ and NT$). Keep the Greek subset for science pages (µ, σ, Δ).
- Files to ship:
  - Literata variable, roman: `opsz 7–72`, `wght 400–800`. Italic variable: same axes (used only for titles of works and scientific names).
  - Fira Sans static: 400, 500, 600, 700 roman; 400 italic.
- `font-display: swap`. Preload only `Literata roman` and `Fira Sans 600` on article pages (headline and key figures).
- Fallback stacks, tuned so the swap moves little:

```css
--font-serif: "Literata", Georgia, "Times New Roman", serif;
--font-sans:  "Fira Sans", "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif;
```

## Type scale

Base size is 16px (1rem). Sizes are in px for readability here; tokens use rem. Line heights are unitless.

| Token | Use | Family, weight | Size (desktop / phone) | Line height | Tracking | Notes |
|---|---|---|---|---|---|---|
| `type.wordmark` | Masthead wordmark | Literata 800 | 30 / 26 | 1 | −0.015em | `opsz 48`, crimson |
| `type.display` | Home page lead headline | Literata 700 | 40 / 28 (fluid) | 1.10 | −0.015em | `opsz 60`, `text-wrap: balance` |
| `type.headline-xl` | Article headline (h1) | Literata 700 | 44 / 30 (fluid) | 1.08 | −0.018em | `opsz 60`, `text-wrap: balance` |
| `type.headline-l` | Section heading on home, h2 in articles | Literata 700 | 23 / 21 | 1.25 | 0 | |
| `type.headline-m` | Secondary stories on home | Literata 700 | 21 / 20 | 1.20 | 0 | `text-wrap: balance` |
| `type.headline-s` | Story headlines in home rows, hub lists | Literata 650 | 18.5 / 18 | 1.25 | 0 | `text-wrap: balance` |
| `type.standfirst` | Article standfirst (dek) | Literata 400 | 20 / 18 | 1.45 | 0 | Full ink colour |
| `type.lead-summary` | Home lead summary | Literata 400 | 18.5 / 17.5 | 1.50 | 0 | |
| `type.body` | Article prose | Literata 400 | 17.5 / 17 | 1.65 | 0 | Max 66 characters per line |
| `type.body-s` | Story summaries, note text | Literata 400 | 15.5 / 15 | 1.55 | 0 | Secondary ink in lists |
| `type.fly` | Fly title above headlines | Fira Sans 600 | 15 | 1.30 | 0 | Crimson, sentence case |
| `type.chart-title` | Chart and table titles | Fira Sans 700 | 18 / 17 | 1.30 | 0 | Full sentence, no full stop |
| `type.chart-subtitle` | Chart subtitles | Fira Sans 400 | 14.5 / 14 | 1.40 | 0 | Secondary ink |
| `type.figure` | Key figures (stat values) | Fira Sans 600 | 28 / 24 | 1.15 | 0 | Lining figures |
| `type.figure-s` | Small figures in home rows | Fira Sans 600 | 24 / 22 | 1.00 | 0 | |
| `type.ui` | Navigation, buttons, table cells | Fira Sans 400–500 | 15 / 15 | 1.45 | 0 | |
| `type.label` | Chart labels, meta lines, stat labels | Fira Sans 400 | 13.5 / 13 | 1.35 | 0 | Tabular figures in charts |
| `type.source` | Source lines, captions, credits | Fira Sans 400 | 12.5 | 1.40 | 0 | Tertiary ink |

The fluid sizes use `clamp()`:

```css
--fs-display:     clamp(1.75rem, 1.2rem + 2.2vw, 2.5rem);   /* 28 → 40 */
--fs-headline-xl: clamp(1.875rem, 1.2rem + 2.6vw, 2.75rem); /* 30 → 44 */
```

## Rules

### Case and emphasis
- **Sentence case everywhere**: headlines, fly titles, chart titles, buttons, navigation. Proper nouns keep their capitals.
- **No all-caps labels.** The only uppercase is in acronyms (GDP, IMF, CPI) and tickers.
- **No mixed emphasis in headlines.** Never set one word in italic, bold or colour.
- Italic is for titles of reports and papers ("*World Economic Outlook*") and scientific names (*Escherichia coli*). Never for emphasis in headlines.
- Bold in body text is rare: at most one short phrase per section, for a defined term the first time it appears.

### Numerals
- **Fira Sans for every number that matters**: key figures, chart values, axis ticks, table cells.
- `font-variant-numeric: tabular-nums lining-nums` on axis ticks, table columns, chart value labels and anything that aligns vertically.
- Key figures shown alone use lining figures (`lining-nums`); proportional spacing is fine there.
- Numbers inside Literata body text keep Literata's default figures; never switch fonts mid-sentence.
- The minus sign is `−` (U+2212), not a hyphen. See [content-style.md](content-style.md) for the full number style.

### Line length and rhythm
- Body text: 60–66 characters per line (`max-width: 66ch`).
- Standfirst and summaries: up to 58 characters per line.
- Headlines: let `text-wrap: balance` even out the lines. Avoid one-word last lines.
- Paragraph spacing: 14px between paragraphs in prose (no first-line indents).
- Headline to standfirst: 12px. Standfirst to meta: 12px.

### Optical sizes (Literata `opsz`)
- Display and article headlines: `opsz 60`.
- Wordmark: `opsz 48`.
- Section headings and smaller headlines: automatic (`font-optical-sizing: auto`).
- Body: automatic.

### Weights in use
- Literata: 400 (body), 650 (small headlines), 700 (headlines), 800 (wordmark only).
- Fira Sans: 400 (labels, body UI), 500 (navigation), 600 (fly titles, key figures, emphasised table rows), 700 (chart titles).
- Do not introduce other weights.

### Links
- In prose: underlined, 1px, offset 3px, in the same ink as the text. Hover thickens the underline. No colour change.
- Glossary terms: a dotted 1px underline in tertiary ink, offset 4px. Hover or tap shows the definition.
- Headline links: no underline at rest; underline on hover.

## Text colours

| Role | Token | Light | Dark |
|---|---|---|---|
| Primary text, headlines, figures | `color.ink` | `#181818` | `#EDEDEA` |
| Secondary text: summaries, subtitles, labels | `color.ink-2` | `#555551` | `#B4B4B0` |
| Tertiary text: source lines, meta, axis ticks | `color.ink-3` | `#656561` | `#9C9C97` |
| Fly titles, wordmark | `color.brand` | `#C8213A` | `#F4687C` |

Contrast for every pairing is listed in [color.md](color.md).
