# Components

The visual spec for every component family the library needs. The component library (built with Opus 5.5 on `@openuidev/react-ui`) implements these; page agents only choose and order them. Names are suggestions for the library (PascalCase).

Every component:
- uses tokens only (no literal colours, sizes or fonts),
- works at 360px wide and in both themes,
- has no outer box, shadow or radius unless this document says so,
- takes data references, not inline numbers, and records the source IDs it shows.

Shared conventions: type styles from [typography.md](typography.md), spacing steps from [layout.md](layout.md#spacing-scale), colours from [color.md](color.md).

---

## Site chrome

### Masthead (`Masthead`)
- One row: wordmark "News" (`type.wordmark`, `color.brand`), section links (Fira Sans 500, 15px, ink), and "Search" with a 15px magnifier icon at the right (ink-2).
- 16px top and 14px bottom padding; a hairline rule underneath.
- Current section: a 2px `color.brand` underline. Hover: a 2px `color.rule` underline.
- On phone the links wrap to a second row under the wordmark. Not sticky.
- The wordmark is placeholder text, set in Literata 800. A proper wordmark comes with the final name.

### Skip link
- "Skip to content", the first focusable element. Hidden until focused, then shown at the top left on `color.ink` with `color.paper` text.

### Date line and region filter (`DateLine`, `RegionFilter`)
- Home only. Left: the date ("Wednesday 6 November 2024", Fira 13.5, ink-2). Right: region links ("All regions", "North America", "Europe", "India", "China", "Asia", "Australia", "Global").
- Selected region in ink, weight 600, with a 2px `color.brand` underline; others ink-2. Hairline rule underneath.
- On hubs, the region filter sits under the hub head instead.

### Footer (`Footer`)
- `color.wash` background, hairline rule on top. Wordmark (24px), a row of links (How pages are made, Glossary, Data access and MCP, RSS feeds, Report an error, Privacy), then the description, accuracy note and licence lines (Fira 13, ink-2).

---

## Article head

### Fly title (`FlyTitle`)
- Fira Sans 600, 15px, `color.brand`, sentence case, 2–4 words. Sits directly above the headline (12px gap).

### Headline (`Headline`)
- `type.headline-xl` on articles; `type.display`, `headline-m` or `headline-s` in lists. `text-wrap: balance`. Linked headlines show an underline only on hover.

### Standfirst (`Standfirst`)
- `type.standfirst`, full ink, max 58ch.

### Byline and meta (`PageMeta`)
- Three short lines in Fira 13.5, ink-2, 3px apart:
  1. "Built with Claude (AI) · Automated checks · Run by Karthik" (Karthik links to the author's site; the line links to the methodology page).
  2. "Data as of 1 November 2024." plus the fiscal period note when useful.
  3. Categories, entity and region as plain text links, 14px apart.
- When updated: a fourth line, "Updated on 3 December 2024", in ink, linking to the change note in the sources panel.

### Disclaimer (`Disclaimer`)
- "Not investment advice." or "Not medical advice." in Fira 13, ink-3, at the end of the sources panel. Never a banner or a box.

---

## Data components

### Key figures (`KeyFigures`)
- 2–4 figures in a row (2 across on phone). Each: a 1px ink rule on top, 10px padding above.
  - Label: Fira 13.5, ink-2 ("Net sales").
  - Value: `type.figure`, ink, lining figures ("$391.0bn"), with a dotted 1px `ink-3` underline that shows it can be tapped for its source.
  - Change: Fira 13, ink-2, in words ("Up 2.0% on fiscal 2023"). Never coloured.
- One source line under the whole row.
- Clicking or tapping a value (or Enter or Space on it) opens a **citation popover**.
- Variant `KeyFigure` with sparkline: the sparkline (120×36) sits to the right of the value.

### Citation popover (`SourcePopover`)
- Opens on click, tap, Enter or Space on any cited number; never on hover or focus alone (Karthik, 16 Nov 2026). Paper background, 1px `color.rule` border, no shadow, 12px padding, max 320px wide.
- Sits under the figure, or above it when there is no room under; always wholly on screen. When it fits neither under nor above (a landscape phone), it opens on the side with more room, its height capped to that side, and scrolls inside so "Open the source" stays reachable.
- Content: the exact value as reported ("$391,035 million"), the document ("Apple Inc. Form 10-K, fiscal 2024"), the location (page, table or XBRL tag), the retrieval date, and a link "Open the source".
- Escape or tapping outside closes it.

### Chart figure (`ChartFigure`)
- The wrapper for every chart and data table: brand rule and tag, title, subtitle, legend, plot, source line, actions. Fully specified in [charts.md](charts.md#anatomy).

### Data table (`DataTable`)
- Also used inside "Show the data table" toggles. Spec in [charts.md](charts.md#tables-as-charts).

### Downloads row (`DataLinks`)
- "CSV", "JSON", "Markdown" as text links, Fira 13.5, ink-2, underlined.

### Comparison table (`ComparisonTable`)
- Guidance vs actual, this quarter vs last, country vs country. Same table style; the compared columns side by side; the change column last, in words or signed figures, never coloured.

### Decision (`PolicyDecision`)
For central-bank and government decisions.
- No box. A 2px ink rule on top, then:
  - The decision as a figure: "6.50%" (`type.figure`) with the label "Policy repo rate" and the change in words: "Unchanged", "Cut by 25 basis points".
  - Up to three supporting facts in a row of small key figures (vote split, stance, next meeting date).
  - The step-line rate path chart below, when there is history.

### Finding (`Finding`)
For science pages.
- A 2px ink rule on top; the finding stated in one sentence in Literata 600, 19px; the key number (if any) as a figure; the method in one line (Fira 13.5, ink-2: "Peer-reviewed paper, 1,204 participants, randomised controlled trial"); the source type as a badge.
- Several findings stack with 24px between them.

### Timeline (`Timeline`)
- A vertical list. Each event: the date in Fira 600, 13.5, ink (left column, 7em wide on desktop, above the text on phone); the event in Literata 16; an optional source link.
- A 1px `color.rule` vertical line connects the events; event dots are 8px circles in `ink`, and the key event's dot is crimson.
- Horizontal timelines only inside charts.

### Map (`MapFigure`)
- A chart; see [charts.md](charts.md#maps).

---

## Text components

### Prose (`Prose`)
- `type.body`, max 66ch, 14px between paragraphs. Section headings `type.headline-l`, 28px above, 10px below.
- Lists: 20px indent, 6px between items. No decorative bullets: default discs in ink-2.
- Each section heading gets an id from its words ("section-why-sales-fell") for links to the section. Ids are unique on the page: a second heading with the same words gets "-2", a third "-3", in document order, so the same page always gets the same ids.

### Glossary term (`GlossaryTerm`)
- A dotted 1px `ink-3` underline. Click, tap, Enter or Space opens a popover (same style and placement as the citation popover) with the definition and a link to the glossary entry; hover and focus alone do not.

### Quote (`Quote`)
For official statements, press-conference remarks and report text.
- Literata 400, 20px, ink, line height 1.45, indented 20px with a 2px ink rule on the left. No large decorative quotation marks; normal quotation marks around the text.
- Attribution below: Fira 13.5, ink-2: "Jerome Powell, Federal Reserve chair, press conference, 18 September 2024", linking to the source.
- Quotes are short (one to three sentences) and only from primary sources.

### Note (`Note`)
For commentary and context in Claude's own words.
- `color.wash` background, 16px 20px padding, no border, no radius.
- Heading in Fira 600, 15px ("Why net income fell while sales rose"); text in Literata 16, line height 1.6; attribution line in Fira 12.5, ink-3 ("Context written by Claude from the 10-K. Figures are from the filing.").
- At most two notes per page.

### Method summary (`MethodSummary`)
- For science and major-report pages. A short definition list: Source type, Study design, Sample, Period, Limitations as stated by the authors. Labels Fira 13.5 ink-2; values Literata 16.

---

## Media

### Image with credit (`ImageFigure`)
- The image at full column width (or its natural width if smaller), no border, no radius, no shadow.
- Caption below: one sentence in Fira 13.5, ink-2; then the credit and licence in Fira 12.5, ink-3 ("NASA/JPL-Caltech. Public domain.").
- **Fallback when the image fails to load**: a `color.wash` box at the same aspect ratio showing the alt text and the credit, centred, in Fira 13.5, ink-2. The layout never shifts. No JavaScript: the text sits on a layer behind the image, so it shows through a blocked or broken image in every engine (Safari draws its small broken-image mark on top) and also while a slow image is still loading; Chromium and Firefox also draw the same text over the broken image itself. The gallery uses the same frame.
- Images are only used when they carry information (a spacecraft, a chart from a paper, an official photo of a facility). Never decoration.

### Gallery (`ImageGallery`)
- 2 columns on phone, 3 on desktop, 8px gaps, square crops only when the source allows. Each image opens full size in a simple overlay with its caption and credit. No carousels.

### Video (`VideoEmbed`)
- Click-to-load. Before loading: a `color.wash` 16:9 box with a play button (a 56px ink circle with a paper triangle) in its centre.
- Caption under the box, always shown: the video title (Literata 600, 18px), the publisher and date (Fira 13.5, ink-2), and the line "Loads from YouTube. YouTube will receive your IP address." in Fira 12.5, ink-3. The text is never inside the 16:9 box, so it is never clipped or covered by the play button at any width, zoom or text size, and it stays readable in forced colours.
- After click: the `youtube-nocookie.com` embed at the same size; the caption stays.
- In print: the caption's title, publisher and date only (no box, no button, no privacy line).
- Video that can't be embedded becomes a link line: "Watch the webcast on the company's site".

---

## Sources

### Sources panel (`SourcesPanel`)
- A 2px ink rule on top; heading "Sources" (`type.headline-l`).
- A list; each item:
  - Title as a link (Fira 500, 15px, ink).
  - Meta row (Fira 13, ink-2): a type badge, the source type ("Filing, SEC EDGAR"), the licence ("Public filing", "CC BY 4.0"), the retrieval date.
- Then the actions row: "Report an error", "Read as Markdown", "Page data (JSON)", "How these pages are made".
- Then the change note (when updated) and the disclaimer and accuracy note.

### Badges (`Badge`)
- Fira 600, 12px, 1px border in the text colour, 3px radius, 1px 7px padding.
- **Primary**: `color.focus` blue text and border. **Context** (news outlets): `ink-3`. **Preprint**: `ink-2` with the word "Preprint". **Peer-reviewed**, **Dataset**, **Press release**, **Filing**: `ink-2`.
- Badges are text first; colour only reinforces.

---

## Lists and navigation

### Lead story (`LeadStory`)
- Fly title, display headline, summary (`type.lead-summary`), meta line (page type, region, data date in Fira 13, ink-3), then a mini chart or key figure under a brand rule with its tag.

### Story item (`StoryItem`)
- Used in home rows, hubs and search results: fly title, headline (`type.headline-s`), an optional key figure (`type.figure-s`) with an optional sparkline, a one-sentence summary (`type.body-s`, ink-2), the page type and region (Fira 13, ink-3).
- In hub lists, a date column on the left (Fira 13.5, ink-3), and items separated by hairlines.

### Section head (`SectionHead`)
- A 2px ink rule; the section name (`type.headline-l`) on the left; a link "All economy pages" on the right (Fira 14, ink-2, underlined).

### Latest list (`LatestList`)
- Home only, under the lead block. The automatic list of the newest 8–10 pages (not curated). A section rule, the label "Latest" (Literata 700, 17px), then one line per page: time today or short date (Fira 13, ink-3, tabular), fly title (Fira 600, 14, crimson) and headline (Literata 16) on one line, wrapping on phones; hairlines between items. Ends with a link "All latest pages".

### Home page slots
- The home page is assembled from a stored curation (see [layout.md](layout.md#home-page-curated-front-page)). `LeadStory`, the side `StoryItem`s and each section's `StoryItem`s receive a page ID and, optionally, the data ID of the figure or chart to show. All text comes from the page record. Components never render curator text.

### On this page (`PageContents`)
- In the rail: a 2px ink rule, heading "On this page" (Literata 700, 17px), then links to each chart and section. Plain text links, no numbering.

### Related list (`RelatedPages`)
- Rail block "More on Apple": up to three pages with a small meta line under each ("Earnings, 1 August 2024"), then a link to the entity hub.

### Pagination (`Pagination`)
- "Newer" and "Older" text links at the ends and page numbers between, Fira 15. Current page in ink, weight 600, with a 2px crimson underline.

### Breadcrumbs
- Not used. Fly titles and category links do this job.

---

## Controls

### Buttons (`Button`)
- **Primary**: ink fill, paper text, Fira 500 15px, 10px 18px padding, 5px radius. Hover: 85% ink. Used once per screen at most (the form submit).
- **Secondary**: paper fill, 1px `color.control-border`, ink text. Used for "Show the data table", "Load video".
- **Text button**: underlined text in ink-2. Used for actions under charts.
- Disabled: 40% opacity and `not-allowed` cursor; always paired with a reason in text nearby.
- Focus: the standard 2px focus ring, 2px offset.

### Forms (`TextField`, `TextArea`, `Select`, `Checkbox`)
- Label above the field: Fira 600, 14px, ink. Hint below the label in Fira 13, ink-2.
- Field: paper fill, 1px `color.control-border`, 5px radius, 10px 12px padding, Fira 400 16px (16px prevents zoom on iOS).
- Focus: border becomes ink plus the focus ring.
- Error: border in `color.error`, an error icon and message under the field in Fira 13.5, `color.error`; an error summary at the top of the form linking to each field.
- The report-an-error form: "What is wrong?" (required, textarea), "Where on the page?" (optional), "Your email, if you'd like a reply" (optional), and a primary button "Send report". After sending: "Report sent. Thank you for checking." with a link back to the page.

### Search field (`SearchField`)
- Full width under the masthead on the search page, 48px tall, Fira 18, with a magnifier icon inside on the left and a "Search" button on the right.
- Filters as a single row of selects below it (Category, Region, Page type, Date).

### Tooltip (`Tooltip`)
- The same look as the citation popover: paper, 1px rule border, no shadow, 12px padding. Appears in ≤120ms, no slide.

---

## States

### Empty state
- A sentence that says what's empty and what to do, in Literata 17, plus one action link. No illustration. Example: "No pages match these filters." / "Clear the region filter".

### Error state
- What happened and what to do, in plain words, plus a link onwards. Example: "Search is unavailable until 00:00 UTC because today's limit was reached." / "Browse by category".

### 404
- The only centred layout: "This page doesn't exist." (Literata 700, 28px), "It may have moved, or the link may be wrong." and a search field plus links to the five categories.

### Loading
- Static pages don't load progressively. Dynamic lists (search results) keep the previous results at 50% opacity while new ones load; no skeletons, no spinners longer than 300ms.

---

## Icons

Very few, all drawn as 1.6px strokes on a 16px grid, in the text colour around them:
search (magnifier), external link (arrow out of a box), download (arrow into a tray), close (×), play (filled triangle), error (circle with !), check (✓), info (circle with i). No icon fonts, no emoji, no decorative icons.
