# Colour

Near-white paper, black ink, one crimson. Blues and a small set of muted colours carry data inside charts. Every value here is a token in [tokens/tokens.json](tokens/tokens.json); [tokens/tokens.css](tokens/tokens.css) is generated from it.

All figures in this document were measured by script, not judged by eye: text contrast with the WCAG 2 ratio; chart palettes with `validate_palette.js` (OKLab ΔE ×100, colour-vision simulation by Machado, Oliveira and Fernandes 2009).

## Interface colours

| Token | Light | Dark | Use |
|---|---|---|---|
| `color.paper` | `#F4F4F1` | `#17181A` | Page background, chart surface |
| `color.wash` | `#E9E9E4` | `#222428` | Note blocks, footer, review UI, row hover |
| `color.rule` | `#DADAD4` | `#313337` | Hairline rules, gridlines |
| `color.ink` | `#181818` | `#EDEDEA` | Headlines, body text, figures, ink rules, baselines |
| `color.ink-2` | `#555551` | `#B4B4B0` | Summaries, subtitles, labels |
| `color.ink-3` | `#656561` | `#9C9C97` | Source lines, metadata, axis ticks |
| `color.brand` | `#C8213A` | `#F4687C` | Wordmark, fly titles, chart rule and tag, highlighted data, current-nav underline |
| `color.on-brand` | `#FFFFFF` | `#17181A` | Text on a crimson fill (rare) |
| `color.control-border` | `#8A8A85` | `#6E6F73` | Input, select and button borders |
| `color.focus` | `#065DA0` | `#8FC0EA` | Focus ring |
| `color.selection` | `#D9EAFC` | `#085491` | Text selection |
| `color.error` | `#C8213A` | `#F4687C` | Form errors only, always with an icon and words |

### The crimson

`#C8213A` is a deep, slightly cool crimson: lighter than the first proposal (`#B8152F`, changed at Karthik's request) and clearly distinct from The Economist's orange-red. In dark mode it becomes `#F4687C` so it stays readable on the dark paper.

Crimson is the brand, not a signal. It never means loss, decline, danger or "bad". See [principles.md](principles.md#3-red-never-means-down).

**Where it appears**: the wordmark; fly titles; the chart rule and its tag; the one highlighted series, bar or value in a chart; the underline of the current navigation item, the selected region filter and the current page in pagination; form errors (with an icon). Nowhere else: not for links, buttons, backgrounds, borders or headings.

### Neutrals

The greys lean very slightly warm-yellow (the paper is `#F4F4F1`, not pure white), so the page reads as paper rather than a screen. Ink is a near-black, not pure black, to soften glare on the near-white paper. In dark mode the paper is a cool near-black (`#17181A`) and text is off-white, never pure white.

## Contrast

WCAG 2 contrast ratios, measured.

| Text token | On paper (light) | On wash (light) | On paper (dark) | On wash (dark) |
|---|---|---|---|---|
| `ink` | 16.11 | 14.58 | 15.15 | 13.25 |
| `ink-2` | 6.80 | 6.15 | 8.54 | 7.47 |
| `ink-3` | 5.31 | 4.81 | 6.44 | 5.64 |
| `brand` | 5.10 | 4.62 | 6.02 | 5.27 |
| `focus` (ring) | 6.19 | 5.60 | 9.22 | 8.07 |
| `on-brand` on `brand` | 5.62 | | 6.02 | |
| `control-border` (non-text) | 3.15 | | 3.54 | |

Every text pairing clears AA (4.5:1) on both surfaces. Rules (`color.rule`) are 1.27:1 (light) and 1.40:1 (dark) by design: they're decorative separators, and no meaning depends on seeing them.

## Chart colours

Charts use colour in one of five ways. Pick the way first, then the colours.

| Job | When | Colours |
|---|---|---|
| **Highlight** (the default) | One thing the title is about, everything else as context | `chart.highlight` + `chart.context` (+ optional `chart.comparison`, `chart.neutral`) |
| **Single series** | One measure, nothing singled out | `chart.cat.1` (blue) for all marks |
| **Categorical** | 2–8 named series that are all equally important | `chart.cat.1` … `chart.cat.8`, in order |
| **Sequential** | Magnitude: heatmaps, choropleth maps | `chart.seq-blue.*` |
| **Diverging** | Above or below a meaningful midpoint | `chart.seq-blue.*` ↔ `chart.div-mid` ↔ `chart.seq-gold.*` |

### Highlight mode (the house default)

Most charts on this site exist to show one finding, so most charts use highlight mode: the subject in crimson, everything else recessive.

| Role | Token | Light | Dark | Use |
|---|---|---|---|---|
| Highlight | `chart.highlight` | `#C8213A` | `#F4687C` | The series, bar or value the title is about. One per chart. |
| Comparison | `chart.comparison` | `#065DA0` | `#4E90D2` | At most one named series to compare against |
| Context | `chart.context` | `#8DAECB` | `#45556A` | All other series or bars |
| Neutral | `chart.neutral` | `#B6B6AF` | `#4C4E53` | Deductions in waterfalls, "Other", missing data |
| Grid | `chart.grid` | `#DADAD4` | `#313337` | Gridlines |
| Baseline | `chart.baseline` | `#181818` | `#EDEDEA` | Zero line, x-axis |

Validation: highlight, comparison and context are distinct for colour-blind readers (worst pair ΔE 16.3 light, 12.8 dark; target ≥ 8) and for everyone (worst ΔE 27.6 light, 21.1 dark; floor 15). Context and neutral sit below 3:1 against the paper on purpose so they recede; because of that, **context and neutral marks are always directly labelled or backed by the data table**, which every chart has.

### Categorical palette

For charts where several series matter equally (shares by country, several companies' margins). Slots are used **in order, never skipped, never cycled**. Crimson is not in this palette: in a categorical chart nothing is highlighted.

| Slot | Name | Light | Dark | Contrast on paper (light / dark) |
|---|---|---|---|---|
| 1 | Blue | `#065DA0` | `#2470B4` | 6.19 / 3.43 |
| 2 | Rust | `#A14F09` | `#B3622A` | 5.24 / 3.97 |
| 3 | Sky | `#05A4D4` | `#0BA5C0` | 2.62 / 6.06 |
| 4 | Green | `#006E30` | `#1B763A` | 5.82 / 3.13 |
| 5 | Violet | `#8463B8` | `#8667B9` | 4.27 / 3.94 |
| 6 | Olive | `#718109` | `#74841B` | 3.93 / 4.28 |
| 7 | Plum | `#8B376B` | `#A83E83` | 6.67 / 3.11 |
| 8 | Gold | `#CD9C34` | `#A17924` | 2.27 / 4.46 |

Validation results:

| Check | Light | Dark |
|---|---|---|
| Lightness band (OKLCH L) | Pass, all 8 in 0.43–0.77 | Pass, all 8 in 0.48–0.67 |
| Chroma floor (C ≥ 0.10) | Pass | Pass |
| Colour-blind separation, neighbouring slots | Pass, worst ΔE 16.9 | Pass, worst ΔE 14.7 |
| Normal-vision separation, neighbouring slots | Pass, worst ΔE 25.9 | Pass, worst ΔE 21.7 |
| First 3 slots, every pair (for scatter, maps, small multiples) | Pass | Pass, worst ΔE 14.8 |
| Contrast ≥ 3:1 on paper | Sky and gold below (relief: direct labels and table) | All pass |

Rules:
- **Lines, bars and stacks**: up to 8 series. Beyond 8, fold the smallest into "Other" (`chart.neutral`).
- **Scatter, bubble, maps and small multiples**: up to 3 categorical series (any two marks can sit side by side there). More than 3: facet into small multiples or fold into "Other".
- **Colour follows the entity, not its rank.** If a filter removes a series, the others keep their colours.
- **Direct labels** on up to 4 series; a legend above the chart for 2 or more series.
- Labels and values are written in ink tokens, never in the series colour.

### Sequential ramps

For magnitude (heatmaps, choropleth maps). One hue, light to dark.

| Step | `chart.seq-blue` | `chart.seq-gold` |
|---|---|---|
| 100 | `#D9EAFC` | `#F4E6CE` |
| 200 | `#AECEF1` | `#E3C697` |
| 300 | `#7FAFE2` | `#CCA45E` |
| 400 | `#4E90D2` | `#B28324` |
| 500 | `#2672B7` | `#906600` |
| 600 | `#085491` | `#6C4C02` |
| 700 | `#003967` | `#4B3301` |

- Blue is the default. Gold is used only when a second magnitude scale appears on the same page, or as the warm arm of a diverging scale.
- **Light mode**: 100 means near zero, 700 means most.
- **Dark mode reverses the anchor**: 700 means near zero (it sinks into the dark paper), 100 means most.
- For discrete ordered marks (tiers, bands), use steps 300–700 in light mode (the lightest still clears 2:1 on paper) and 100–500 in dark mode. Both were validated as ordinal ramps: monotone lightness, step gaps ≥ 0.06, single hue.

### Diverging scale

For values above and below a meaningful midpoint (change from a year earlier, deviation from target).

- **Blue arm** (`seq-blue` 200–700) for one side, **gold arm** (`seq-gold` 200–700) for the other, with **`chart.div-mid`** (`#E4E4DF` light, `#3A3C40` dark) at the midpoint. Equal steps on each arm.
- Which side is blue is fixed per measure, not by good or bad: by convention, blue for above the midpoint, gold for below.
- **Never crimson and never green/red** in a diverging scale.

### Status colours

There are none for data. The site does not mark values as good or bad. The only status colour is `color.error` for form validation, which always comes with an icon and a sentence.

## Images

Images are hotlinked from their sources and can't be recoloured. Their placement rules are in [components.md](components.md#image-with-credit-imagefigure). In dark mode images are shown as they are (no dimming filters), on the dark paper with no frame.

## Changing a colour

1. Change the value in `tokens/tokens.json` (both light and dark).
2. If it is a chart colour, re-run the validator for both modes:
   `node validate_palette.js "<all 8 slots>" --mode light --surface "#F4F4F1"` and `--mode dark --surface "#17181A"`, plus `--pairs all` on the first three slots.
3. If it is a text colour, check its contrast on paper and on wash in both modes (≥ 4.5:1).
4. Run `node design/tokens/build.mjs` to regenerate `tokens.css`.
5. Update the tables in this document.
