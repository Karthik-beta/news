# House chart style

Charts are the heart of the site and its most recognisable element. Every chart, whatever its type, looks like it came from the same desk.

Colours are defined in [color.md](color.md#chart-colours); type in [typography.md](typography.md).

## Anatomy

```
▬━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━   brand rule (1px crimson) + tag (36×7px)
Services earns about twice the gross margin of products   title: the takeaway (Fira Sans 700, 18px)
Gross margin as a share of net sales, fiscal 2020–2024, % subtitle: what, when, unit (Fira 400, 14.5px, ink-2)
■ Services  ■ Products                                     legend (only for ≥2 series, above the plot)
 80 ─────────────────────────────────────────            gridlines: solid hairlines, horizontal only
 60 ──────────────────────────────────●  Services 73.9   direct label at the end of the line
 40 ──────────────────────────────────●  Products 37.2
 20 ─────────────────────────────────────────
  0 ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━            baseline in ink
    2020     2021     2022     2023     2024
Source: Apple Inc., Forms 10-K for fiscal 2020 to 2024    source line (Fira 12.5px, ink-3)
Show the data table · CSV · JSON                           actions
```

Every chart is a `<figure>` with, in this order:

1. **Brand rule and tag.** A 1px `color.brand` line across the full width, with a 36×7px crimson tag at the left end. It marks "this is a chart" and is the site's signature.
2. **Title.** The takeaway as a sentence. See [content-style.md](content-style.md#chart-and-table-titles).
3. **Subtitle.** What is plotted, the period and the unit.
4. **Legend**, only when there are two or more series. A row of small keys above the plot, left-aligned: a 10×10px square for bars and areas, a 14×2.5px line for lines. Legend text in `ink-2`.
5. **The plot.**
6. **Source line.** "Source: " and the source names, short form, linking to the sources panel entry.
7. **Actions.** "Show the data table", "CSV", "JSON". Text links in `ink-2`, 13.5px.

Spacing: rule → title 16px; title → subtitle 4px; subtitle → legend or plot 12px; plot → source 10px; source → actions 6px.

## Choosing the form

Pick the chart from what the data is doing. Sometimes the answer isn't a chart.

| The data's job | Form | Notes |
|---|---|---|
| One headline number | **Key figure** (not a chart) | A one-bar chart is never right |
| A few headline numbers | **Key figures row** | 2–4 across; see [components.md](components.md#key-figures-keyfigures) |
| Change over time, one or two series | **Line chart** | Highlight the subject, context the rest |
| Change over time, discrete periods (quarters, years) | **Column chart** | Highlight the latest or the subject period |
| Ranking or comparison of categories | **Horizontal bar chart**, sorted | Long labels fit; best on phones |
| Revenue to profit | **Waterfall** (standard) or **Sankey** (Tier 1) | Totals in comparison blue, deductions neutral, the end result highlighted |
| Part of a whole, ≤ 6 parts | **Tall segmented bar** or horizontal stacked bar | Never a pie or donut |
| Part of a whole over time | **Stacked columns** (≤ 5 parts) or small multiples | |
| Two points in time, several items | **Slope chart** or **dot plot (dumbbell)** | Guidance vs actual, before vs after |
| Distribution | **Dot plot** or **histogram** | |
| Values by country or region | **Choropleth map** (sequential) or **ranked bars** | A map only when geography matters; bars are more precise |
| Many comparable series | **Small multiples** | Same scales in every panel |
| Matrix of two categories | **Heatmap** | Sequential blue, values written in cells when they fit |
| Sequence of dated events | **Timeline** | See [components.md](components.md#timeline-timeline) |
| A published forecast table | **Table**, as published | Never redraw a forecast as our own projection |
| Policy rate path | **Step line** | Steps, not slopes, because rates change on dates |
| Trend inside a list or card | **Sparkline** | No axes; latest value as a dot in highlight |

Never: pie charts, donuts, 3D, radar charts, dual y-axes, gauges, word clouds, bubble charts with more than 3 series, animated charts.

## Marks

| Element | Spec |
|---|---|
| Bars (vertical) | Width set by the band, max 48px; 2px gap of paper colour between adjacent bars and stacked segments; square ends |
| Bars (horizontal) | 22px thick; 8px between rows; square ends; value label just past the bar end |
| Lines | 2.5px for the highlight and comparison; 1.5px for context lines; round joins and caps |
| End markers | 8px circle on the last point of each labelled line, in the line's colour, with a 2px paper ring |
| Points (dot plots, scatter) | 8px circles, 2px paper ring where they overlap; hit area ≥ 24px |
| Areas | Only for stacked shares; fills at full colour, 2px paper gaps between layers |
| Gridlines | 1px solid `chart.grid`, horizontal only. Never dashed. No vertical gridlines. |
| Baseline | 1.25px solid `chart.baseline` at zero (bars) or the axis bottom (lines) |
| Axis ticks | No tick marks; labels only |
| Annotations | Text in `ink-2`, 13px, with a 1px `ink-3` leader line when needed |
| Event markers | A 1px vertical `ink-3` line with a short label at the top ("Rate cut, 18 Sep") |
| Published forecasts | The same colour as the series, dashed (4px dash, 3px gap), labelled with the forecaster ("IMF projection"). The only dashed lines allowed. |
| Missing data | A gap in a line; a `chart.neutral` hatched bar with "No data" in the table |
| Negative values | Bars extend below the baseline in the same colour. Never recoloured red. |

### Corner radius
Zero. Bars and segments have square ends, like the rest of the site's content.

## Axes and scales

- **Bars and columns always start at zero.**
- **Lines may start above zero** when the variation is the point (inflation between 2% and 4%). The bottom gridline then carries its value, and there is no zero baseline drawn.
- **Y-axis labels** sit on the left, above their gridline, in `ink-3`, 13.5px, tabular figures. The top label may carry the unit when it's a percentage ("80%"); otherwise the unit lives in the subtitle.
- **4–6 gridlines.** Round numbers only (0, 20, 40 … not 0, 18.6, 37.2).
- **X-axis** labels below the baseline: years as "2024", months as "Jan", quarters as the source labels them ("Q3", "Q4 FY24"). Thin out labels on phones (every other year) rather than rotating them. Never rotate text.
- **Log scales** only when the data spans several orders of magnitude, and the subtitle says "log scale".
- **One y-axis per chart.** Two measures with different units become two charts or small multiples, or both are indexed to 100 at a common start and the subtitle says so.

## Labels

- **Direct labels beat legends.** Label up to 4 series directly at their line ends or beside their bars; the legend stays for ≥ 2 series as the key.
- **Value labels selectively**: the highlighted value, the latest value, the extremes, or every bar in a bar chart of ≤ 12 bars. Never a number on every point of a line.
- The highlighted value's label is **bold and in the highlight colour**; every other label is in ink tokens, never in the series colour.
- A label that doesn't fit inside a mark goes outside it; a label that doesn't fit anywhere moves to the tooltip, and the value stays in the data table.
- Labels never overlap marks or each other. Nudge vertically; if that fails, drop the less important label.

## Highlight first

The default chart has one highlighted subject:

- The subject of the title is in `chart.highlight` (crimson).
- One comparison series, if needed, is in `chart.comparison` (blue).
- Everything else is `chart.context` (pale blue-grey) or `chart.neutral` (grey).

Only use the categorical palette when every series matters equally and the title is about all of them. If a chart needs more than 8 colours, it is the wrong chart.

## Specific chart types

### Waterfall (revenue to profit)
- Rows from net sales down to net income. Totals (net sales, gross margin, operating income) in `chart.comparison`; deductions (cost of sales, operating expenses, taxes and other) in `chart.neutral`, floating between their start and end; the final result (net income) in `chart.highlight`.
- Horizontal layout (labels on the left, bars to the right) so it reads on phones. Value labels at each bar's end; deductions shown with a minus sign.
- Labels use the company's own line names.

### Sankey (Tier 1 earnings)
- Revenue segments on the left flowing into total revenue, then splitting into costs and profit.
- Revenue flows in `chart.context`; cost flows in `chart.neutral`; the profit path in `chart.highlight`.
- Node labels in ink with values; flows have no outlines. Minimum flow thickness 2px; flows below 1% of the total fold into "Other".
- On phones, fall back to the waterfall: a Sankey can't be read at 360px.

### Line charts and step lines
- Up to 4 labelled lines; context lines thinner. End labels show the series name (bold, in its colour) and the latest value (in ink).
- Rate paths use step lines with steps at decision dates, and event markers for decisions.

### Bar charts
- Sorted by value unless the categories have a natural order (time, size bands).
- The highlighted bar is crimson; all others are one colour (context, or blue for a single series). Never colour bars by their value.

### Maps
- Equal Earth projection for world maps; national projections for single countries.
- Sequential blue fills; borders as 0.75px lines in the paper colour; no basemap imagery, no terrain, no labels other than those needed.
- Regions without data in `chart.neutral` with hatching, and "No data" in the legend.
- A legend bar with 5–7 steps above the map. Always paired with a ranked table.

### Small multiples
- Same scales and the same size in every panel. Panel titles in Fira Sans 600, 14px.
- 2 columns on phone, 3–4 on desktop.

### Tables as charts
- Tables follow the same anatomy (rule and tag, title, subtitle, source).
- Numbers right-aligned, tabular figures; text left-aligned. Header row in `ink-2`, 13px, with a 1px ink rule below. Rows separated by hairlines. No zebra stripes, no vertical lines.
- The highlighted row: bold text and a 3px crimson inset bar on the left.
- "As reported" tables keep the source's scale and precision.
- Wide tables scroll sideways inside their container, with the first column fixed.

### Sparklines
- 120×36px. Line 2.2px in `chart.comparison`; the latest point as a 4.5px dot in `chart.highlight`. No axes, no labels; the value is written beside it.

## Interaction

Charts are interactive, quietly:

- **Line charts**: a vertical hairline crosshair snaps to the nearest period and shows one tooltip listing every series at that point.
- **Bars, segments, cells, points**: each mark is its own hover and focus target, with a hit area larger than the mark (≥ 24px).
- **Tooltip**: paper background, 1px `color.rule` border, no shadow, 12px padding. Value first (Fira 600, ink), series name and period after (Fira 400, ink-2), then the source in `ink-3` ("Form 10-K, fiscal 2024, p. 28"). Series keyed with a short line in its colour.
- **Keyboard**: the chart is one tab stop; arrow keys move between points; the tooltip follows focus; Escape closes it.
- **Touch**: tap a mark to show its tooltip; tap elsewhere to close.
- No zooming, panning, brushing, animated transitions or entrance animations. The hovered mark may lighten slightly (≤ 120ms).
- Tooltips enhance, they never gate: every value is also in the data table.

## Responsive behaviour

- Charts are re-laid out for narrow screens, not just scaled down. Text in a chart never renders smaller than 12px.
- Horizontal bars and waterfalls are built in HTML/CSS so labels stay full size; line and column charts are SVG with a viewBox sized for the phone layout first.
- On phones: fewer x-axis labels, end labels may move above the line, legends wrap, Sankeys fall back to waterfalls, maps fall back to ranked bars plus a small map.
- Very wide charts (long timelines) scroll sideways inside their own container with a visible fade at the edge.

## Dark mode

Charts use their dark tokens: dark paper, re-stepped categorical colours, the lighter crimson, the reversed sequential anchor. Never an automatic invert.

## Accessibility

See [accessibility.md](accessibility.md#charts). In short: a text summary as the accessible name, a data table toggle, a CSV link, keyboard-reachable tooltips, nothing carried by colour alone.

## Social images

For manual sharing (and later auto-posting), any chart can be exported as a 1200×675 PNG:

- Paper background, the chart's brand rule and tag, title, subtitle, the chart, the source line.
- The wordmark "News" at the bottom right, and the page URL at the bottom left, in `ink-3`.
- Light mode only. 48px padding. Text at least 20px.

## Checklist before a chart ships

- [ ] The title states a finding the chart actually shows.
- [ ] The subtitle names the measure, the period and the unit.
- [ ] One highlight at most; crimson used for nothing else in the chart.
- [ ] Bars start at zero; one y-axis.
- [ ] No legend for one series; a legend plus direct labels for 2–4.
- [ ] No red/green for up/down; negatives in the same colour.
- [ ] Gridlines solid, horizontal, hairline.
- [ ] Source line present; data table and CSV available.
- [ ] Readable at 360px wide; no label collisions; nothing below 12px.
- [ ] Correct in dark mode.
