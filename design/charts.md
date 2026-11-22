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

- The subject of the title is in `chart.highlight` (crimson), or `chart.neutral-highlight` when it fell (below).
- One comparison series, if needed, is in `chart.comparison` (blue).
- Everything else is `chart.context` (pale blue-grey) or `chart.neutral` (grey).

Only use the categorical palette when every series matters equally and the title is about all of them. If a chart needs more than 8 colours, it is the wrong chart.

### When the subject fell

Crimson never marks a subject that fell (Karthik, 16 Nov 2026). A highlighted subject that fell is drawn in `chart.neutral-highlight` (charcoal in light mode, pale grey in dark) with its label bold in the same colour; everything else about it (line weight, end marker, the bold row in the data table) stays the highlight's. The chart code decides from the data, never the page writer:

| Chart | The subject fell when |
|---|---|
| Bar chart | The highlighted bar's value is below zero, or below the comparison value when the chart names one |
| Column chart | The highlighted column's value is below zero, or below the comparison value when the chart names one |
| Line and step-line chart | The highlighted series' last value in the shown range is below its first (gaps skipped; a single point or a flat line did not fall) |
| Data table | A highlighted period: every value in the row is below zero. A highlighted series (rows are series): the line rule |
| Waterfall and Sankey | The result (the last total, usually net income) is below zero: a loss. A Sankey cannot draw a loss as a flow, so it shows its waterfall |
| Dot plot | One value per row: the bar rule. A dumbbell: the highlighted row's second value is below its first |
| Slope chart | The highlighted item's second value is below its first |
| Stacked bar | The highlighted part's value in the last row is below its value in the first row (the line rule, across rows) |
| Segmented bar | The highlighted part is below zero (it cannot be drawn, so it is in the key and the table only) |
| Small multiples | The highlighted panel's series: the line rule. A panel of columns also fell when any of its columns is below zero |
| Histogram | The highlighted item's value is below zero (the bar rule); its whole range takes the colour |
| Heatmap | Never: colour is magnitude, so a heatmap has no highlight |

Today neither the bar nor the column chart takes a comparison value, so for both the rule is "below zero". A missing value never fell. A rise, a flat line and a zero are crimson as usual.

## Specific chart types

### Waterfall (revenue to profit)
- Rows from net sales down to net income. Totals (net sales, gross margin, operating income) in `chart.comparison`; deductions (cost of sales, operating expenses, taxes and other) in `chart.neutral`, floating between their start and end; the final result (net income) in `chart.highlight`.
- Horizontal layout (labels on the left, bars to the right) so it reads on phones. Value labels at each bar's end; deductions shown with a minus sign.
- Labels use the company's own line names.
- Each row says what it does, and its value stays as the source publishes it: a **total** runs from zero (the first row is one; the last total is the result); a **deduction** subtracts its value (a cost published as 210,352 goes down, a tax credit published as −20 goes up); an **addition** adds its value (other income of 269 goes up, −565 goes down). Labels carry the effect's sign ("−$210.4bn", "+$269m"); the data table keeps the published values.
- The running amount is exact. When the lines shown do not add up to a total (a line left out), the bars stay where the source puts them and a line under the chart says so ("The lines shown do not add up to Operating income: the source has lines not shown here."). After a line with no value, the lines up to the next total cannot be placed: they are hatched, with their values.
- The plot is the horizontal bar chart's island, so a waterfall adds no JavaScript.

### Sankey (Tier 1 earnings)
- Revenue segments on the left flowing into total revenue, then splitting into costs and profit.
- Revenue flows in `chart.context`; cost flows in `chart.neutral`; the profit path in `chart.highlight`.
- Node labels in ink with values; flows have no outlines. Minimum flow thickness 2px; flows below 1% of the total fold into "Other".
- On phones, fall back to the waterfall: a Sankey can't be read at 360px.
- Built from the same rows as the waterfall (totals, deductions, additions) plus optional revenue segments that add up to the first total, so the two can never tell different stories. Nodes: totals in `chart.comparison` and the result in `chart.highlight`, segments and additions in `chart.context`, costs in `chart.neutral`. Each column runs additions, then the total, then the costs, so the profit path stays along the top and no two flows cross. Nodes are the hover and keyboard stops; a flow shows its node's tooltip.
- **Folding.** Siblings under 1% of total revenue (segments, the costs leaving one total, the additions joining one) fold into one "Other" node in `chart.neutral` when two or more are that small; a single small flow keeps its name and is drawn at the 2px minimum. "Other" writes no number on the plot (its sum would be ours, not the source's); its tooltip lists each line folded into it, and the data table lists every line.
- **When it shows.** The Sankey needs its plot to be at least 7em for each label gutter plus 7.5em per gap between columns, measured in the plot's own text (a container query, so it also holds at 200% text). Narrower, which includes every phone, the waterfall of the same rows shows instead, by CSS alone; both are rendered on the server. With five columns (segments, revenue, gross margin, operating income, net income) the Sankey shows from a 44em plot, about 600px. The widths are written for 2 to 10 columns, the most 16 rows can make.
- **When it can't flow.** A loss, a missing or unknown value, segments that do not add up to revenue, or totals the lines above them do not add up to: the waterfall shows at every width.
- Labels are nudged apart in each column so none overlaps another, and the plot grows when they need more height. Labels between columns sit on a paper ground so they read over the flows.

### Stacked and segmented bars (part of a whole)
- **Stacked bar:** one 22px bar per row, each split into the same 2–8 parts: the categorical colours in the given order, or one highlighted part in crimson and the rest context. A legend names the parts; a row's total, when the source publishes one, is written past the bar's end; every part's value is in the tooltip and the table. A negative part cannot be stacked: it is left out of the bar and kept in the tooltip and the table.
- **Segmented bar:** one 28px bar for one whole in 2–6 parts. Under the bar a key lists every part in block order with its value, so no label squeezes inside a narrow block; the key is the legend. Each part is its own hover and keyboard stop.
- Square ends and a 2px paper gap after every segment, as in stacked columns.

### Dot plots and dumbbells
- One row per item: 8px dots with a 2px paper ring on a shared scale that need not start at zero (positions, not lengths), so there is no axis and every dot carries its value. A hairline guide runs along each row.
- One value per row: dots in `chart.cat.1`, or the highlighted row in crimson and the rest context. Two values (a dumbbell: before and after, guidance and actual): the first in `chart.context`, the second in `chart.comparison`, joined by a 2px context line; the highlighted row's second dot is crimson. The legend names the two values. The smaller value's label sits left of its dot, the larger's right.

### Slope charts
- One line per item from its first value to its second, on one scale that need not start at zero, with 8px end dots. Values on the left; values and names on the right, so no legend is needed. The two points' names sit above the ends.
- A tight slope (narrower than 20em of its own text; see "Large text" below) has no room for both gutters: the right-hand labels keep only their values, in a gutter as wide as the widest of them, and the lines take the rest of the width. Each item's name and both values ("Europe $94.3bn to $101.3bn") move to a list below the plot, in the order of the right-hand values, the highlight's name bold and its value in its colour. The two points' names sit at the plot's edges.
- The highlighted item is crimson (2.5px) and the rest context (1.5px); with nothing highlighted, up to 8 items take the categorical colours in order and any beyond are neutral. Labels on each side are nudged apart, and the plot grows when they need more room. A pointer picks the nearest line.

### Line charts and step lines
- Up to 4 labelled lines; context lines thinner. End labels show the series name (bold, in its colour) and the latest value (in ink).
- Rate paths use step lines with steps at decision dates, and event markers for decisions.

### Bar charts
- Sorted by value unless the categories have a natural order (time, size bands).
- The highlighted bar is crimson (the neutral highlight when it fell); all others are one colour (context, or blue for a single series). Never colour bars by their value otherwise.

### Maps
- Equal Earth projection for world maps; national projections for single countries.
- Sequential blue fills; borders as 0.75px lines in the paper colour; no basemap imagery, no terrain, no labels other than those needed.
- Regions without data in `chart.neutral` with hatching, and "No data" in the legend.
- A legend bar with 5–7 steps above the map. Always paired with a ranked table.

### Small multiples
- Same scales and the same size in every panel. Panel titles in Fira Sans 600, 14px (the chart-subtitle size, 14.5px; the highlighted panel's bold).
- 2 columns on phone, 3–4 on desktop.
- Each panel is a line or column chart's own plot, one series each, 2–12 panels. They share one x axis (every panel's periods, so a period a panel lacks is a gap or a hatched column) and one value axis (every panel's values), and line panels keep the end-label room of the widest so every plot is the same width. Plots are shorter than a single chart's (8rem on phones, 9rem wider) and usually tight, so end labels sit below the x axis.
- With nothing highlighted every panel is blue. Naming one series makes its panel the subject (crimson, or the neutral highlight when it fell) and the rest context.
- Columns per row: 2 on phones; 3 from a 34em figure and 4 from 42em, except where that leaves a short last row (4 panels go 2 then 4 across; 5, 6 and 9 stay at 3).

### Histograms
- Ranges are half-open, [a, b), on round numbers (1, 2 or 5 × 10ⁿ), about log₂(n) + 1 of them for n values (Sturges), at most 12. Edges are worked out from the exact decimals, so a value on an edge always goes up a range.
- The column chart's plot, each column filling its range with the 2px paper gap; columns from zero; the x labels are the edges; the y axis counts values in whole numbers.
- No count is written on the plot: a count is ours, not the source's. The tooltip lists the range's values with their citations (up to 8), and the data table lists every value, lowest first.
- Every range is blue; naming one item makes its range the subject and the rest context. Values with no data, or in another unit, are not counted and are named under the plot.

### Heatmaps
- One row per series (its name on the left, in the value axis's place), one column per period (labels below, thinned on phones like any x axis); best up to about 24 periods; on phones keep heatmaps to about 12 periods, as more make cells too thin to read. For magnitudes that are never below zero (levels, shares, counts): the ramp reads least to most, so on signed data (changes) zero would be a middle shade and every fall a pale one; signed data goes in a line or column chart.
- Classes are the round-number ranges of a value axis over the data (3 to 5; one, the range holding it, when every value is equal), each a fixed step of the sequential blue ramp: in light mode 300, 400, 500, 600, 700 for least to most; in dark mode 600, 400, 300, 200, 100 (500 is skipped: no text colour reads on it in dark). Fewer classes take steps spread across the ramp, the ends always used. Both ramps pass the dataviz validator's ordinal checks on paper.
- Values are written in the cells, in ink on the light steps and in the paper colour on the dark ones (each at least 4.5:1), when every column has room for the longest; the plot is a query container, so values give way at large text sizes. They are always in the tooltip and the data table.
- A legend above the grid names each range ("60 to 70"), and "No data" with a hatched key when a cell is hatched.
- Square-cornered cells at least 24px tall with 2px paper gaps; each is a hover and tap target. One tab stop: left and right step through the cells, up and down move a row.

### Tables as charts
- Tables follow the same anatomy (rule and tag, title, subtitle, source).
- Numbers right-aligned, tabular figures; text left-aligned. Header row in `ink-2`, 13px, with a 1px ink rule below. Rows separated by hairlines. No zebra stripes, no vertical lines.
- The highlighted row: bold text and a 3px crimson inset bar on the left (`chart.neutral-highlight` when its subject fell).
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
- **Large text.** A plot narrower than 20em of its own text is *tight*: a phone with the reader's text at 150–200%, which no viewport media query sees, so line, step-line, column and slope plots are CSS query containers and switch on their width in em. A tight plot keeps fewer x labels (chosen by code for about 16 characters of room), lets each wrap to half the way to its neighbours, and pins the first and last inside the plot; line end labels move to a row below the x axis ("Services 73.9%", or the period and the value for one series) so the lines take the full width; a step line's one event label wraps between its line and the plot's edge, in a row above the plot that grows with it; a tight column chart shows only the highlighted and latest values, the first and last aligned to their column's outer edge. The x axis and the event row grow to their tallest label, so a label that wraps to three lines never runs into the text beside it. Nothing leaves the plot and the page never scrolls sideways. At 100% text no phone plot is tight.
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
- [ ] One highlight at most; crimson used for nothing else in the chart, and never for a subject that fell.
- [ ] Bars start at zero; one y-axis.
- [ ] No legend for one series; a legend plus direct labels for 2–4.
- [ ] No red/green for up/down; negatives in the same colour.
- [ ] Gridlines solid, horizontal, hairline.
- [ ] Source line present; data table and CSV available.
- [ ] Readable at 360px wide; no label collisions; nothing below 12px.
- [ ] Correct in dark mode.
