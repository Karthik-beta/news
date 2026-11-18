# Design principles

Nov 14, 2026 · Agreed with Karthik · Direction E ("Economist school")

These principles decide every case the other design documents don't cover. When two rules conflict, the one higher in this list wins.

## Who the design is for

A global, English-reading, technically and financially literate reader who wants the substance of a report in minutes, with every number visible and its source one tap away. They read on a phone and on a laptop in equal measure. AI tools that fetch and cite pages are secondary readers; they get the same content as Markdown and JSON, so the visual design serves people first.

## Personality

**A meticulous analyst.** Precise, dry, confident. The numbers do the talking. The site never sells, never hypes, never predicts.

It looks like serious financial journalism (The Economist and the Financial Times are the reference points) without copying either: its own crimson, its own fonts, its own wordmark.

## The principles

### 1. The data is the design

Charts, key figures and tables are the content, not illustrations of it. Every visual device must carry information. If removing an element loses no information, remove it.

- No stock photos, decorative illustrations, gradients, textures or background patterns.
- No shadows, glows or rounded "cards" around data. Rules (thin lines) and spacing do the structuring.
- No motion that is not a response to the reader (see [Motion](#9-almost-no-motion)).

### 2. One colour, used sparingly

Crimson is the site's only brand colour. It appears in exactly these places:

1. The wordmark.
2. The topic line ("fly title") above a headline.
3. The rule and tag across the top of every chart.
4. The one highlighted series, bar or value in a chart.
5. Small interface underlines: the current navigation item, the selected region filter and the current page in pagination.
6. Form errors, always with an icon and words.

Everything else is ink on paper, plus blues and the other data colours inside charts. A page where crimson appears more than about six times above the fold has too much.

### 3. Red never means "down"

In finance, red usually signals a loss. On this site crimson only means "this is the thing to look at". Rises and falls are written in words or with neutral signs ("Up 2.0%", "−5.7%"), in ink colours. Green and red are never used to show good and bad, up and down, or gain and loss.

### 4. Every number shows where it came from

Trust is the brand. Citations are quiet but always present:

- A source line sits under every chart, table and group of key figures.
- Every key figure can be clicked or tapped to show its exact source (document, page or XBRL tag, retrieval date).
- Every page ends with a sources panel that labels primary sources and linked context.
- Downloads (CSV, JSON, Markdown) are always one click away.

### 5. Facts and commentary look different

Facts sit on the paper. Commentary and context (anything written in Claude's own words that explains rather than reports) sit in a tinted note block with a small line saying who wrote it. A reader should never have to guess which is which.

### 6. Takeaway titles, plain subtitles

Every chart has a title that states the finding in a full sentence ("Services was the only large category to grow by double digits") and a subtitle that says exactly what is plotted, over what period, in which units. Titles report; they never predict, recommend or judge.

### 7. Numbers exactly as the source states them

Original currency, original units and the source's own period labels (fiscal years and quarters as the company names them). No conversion. Formatting for readability (thousands separators, "bn") is allowed; changing the unit system or currency is not. See [content-style.md](content-style.md).

### 8. Calm, balanced density

A balanced page: dashboard sections with room to breathe, separated by short passages of text. Not a wall of numbers, not one idea per screen. A reader should be able to scan the key figures in seconds and read the whole page in a few minutes.

### 9. Almost no motion

- No entrance animations, parallax, auto-playing media, carousels or scroll-triggered effects.
- Allowed: short (≤200 ms) transitions that confirm a reader's action, such as a tooltip appearing, a section expanding or a theme switching.
- `prefers-reduced-motion` turns even those off.

### 10. Built to survive 250,000 pages

Pages are laid out by AI from a component library. The design must hold up in layouts nobody drew in advance. So:

- The look comes from tokens (colour, type, spacing) and from the components, never from page-specific styling.
- Every component looks right on its own and next to any other component.
- There is one way to do each thing. One chart style, one key-figure style, one note style.

### 11. Light first, dark done properly

Light mode on near-white paper is the default. Dark mode follows the reader's device setting and has its own selected colours (a dark paper, a lighter crimson, re-stepped data colours), not an automatic inversion.

### 12. Accessible by default

WCAG 2.2 AA is the floor. Meaning never depends on colour alone. Every chart has a text summary and a data table or CSV. See [accessibility.md](accessibility.md).

## What we never do

| Never | Because |
|---|---|
| Stock photos, AI-generated images or decorative illustrations | Cheap-looking, and they carry no information |
| Gradients, glows, drop shadows, glassmorphism | Template look; decoration over data |
| Red/green for down/up or bad/good | Crimson is brand only; colour-blind readers lose the meaning |
| Rounded cards with shadows around every block | Flattens hierarchy and looks like a SaaS dashboard |
| ALL-CAPS labels and tracked-out eyebrows above every heading | A generic template tell |
| Emoji or icons as section markers | Decoration, not information |
| Dual-axis charts | Misleading; use two charts instead |
| Legends that could be direct labels | Make the reader decode colours |
| Our own forecasts, ratings or recommendations | Out of scope (PRD non-goals) |
| Copying The Economist's or the FT's logos, exact colours or licensed fonts | We borrow conventions, not brand assets |

## Where each decision lives

| Topic | Document |
|---|---|
| Colours, data palettes, contrast | [color.md](color.md) |
| Fonts, type scale, numerals | [typography.md](typography.md) |
| Grid, spacing, breakpoints, page templates | [layout.md](layout.md) |
| House chart style | [charts.md](charts.md) |
| Components | [components.md](components.md) |
| Words, numbers, dates, units | [content-style.md](content-style.md) |
| Accessibility | [accessibility.md](accessibility.md) |
| Tokens | [tokens/](tokens/) |
| Rules for the page-writing agents | [skills/news-brand](../skills/news-brand/SKILL.md) |
