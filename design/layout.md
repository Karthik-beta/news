# Layout

The page is near-white paper divided by thin rules and spacing. There are no boxes around content, except the tinted note block and the review-only UI. Text is left-aligned everywhere; nothing is centred except the 404 page message.

## Breakpoints

Mobile-first. Design for 360px wide first, then widen.

| Name | Width | What changes |
|---|---|---|
| `phone` | < 600px | One column. Key figures 2 across. Chart labels move above bars. Rail content moves below the article. |
| `tablet` | 600–979px | One reading column, wider. Key figures 4 across. Home rows 2–3 across. |
| `desktop` | 980–1119px | Article gets its right rail. Home lead becomes two columns. |
| `wide` | ≥ 1120px | Container stops growing at 1120px; extra space becomes margin. |

## Container and gutters

- Max content width: **1120px**, centred.
- Side gutters: **16px** on phone, **24px** on tablet, **32px** on desktop, always at least the device's safe-area inset:
  `padding-inline: max(var(--gutter), env(safe-area-inset-left/right))`.
- The page never scrolls sideways. Only tables and very wide charts may scroll, inside their own container.

## Spacing scale

A 4px base. Use only these steps (tokens `space.*`).

| Token | px | Typical use |
|---|---|---|
| `space.1` | 2 | Hairline gaps between stacked labels |
| `space.2` | 4 | Label to value inside a key figure |
| `space.3` | 8 | Gap between bars, between tags |
| `space.4` | 12 | Headline to standfirst; chart title to chart |
| `space.5` | 16 | Between key figures; inside notes |
| `space.6` | 20 | Card-less list item padding |
| `space.7` | 24 | Between stories in a row (vertical) |
| `space.8` | 28 | Between sections of an article |
| `space.9` | 32 | Between stories (horizontal); section top padding |
| `space.10` | 40 | Between home blocks |
| `space.11` | 56 | Article column to rail |
| `space.12` | 72 | Page bottom padding before the footer |

Lay out groups with flexbox or grid `gap`, not margins on individual elements.

## Rules (lines)

Rules carry the structure. Four kinds, nothing else:

| Rule | Token | Use |
|---|---|---|
| Hairline | 1px `color.rule` | Between stories, list items, table rows; under the masthead |
| Ink rule | 1px `color.ink` | Top of each key figure; under table headers |
| Section rule | 2px `color.ink` | Top of each home section, the sources panel and each rail block |
| Brand rule | 1px `color.brand` + 36×7px tag at the left | Top of every chart and table; top of the home lead figure |

No vertical rules except the single hairline between the home lead and its side column on desktop.

## Corners, borders, shadows

- Corner radius: **0** for content (charts, notes, images, tables). **3px** for small interactive chips and badges; **5px** for buttons and inputs.
- No shadows, anywhere.
- Borders only on form inputs, buttons, badges and the focus ring.

## Page templates

The fixed base from the PRD appears on every article page: fly title, headline, standfirst, byline, "Data as of", categories, key figures (where the page type has them), sources panel, "Report an error", Markdown and JSON links, and "Updated on …" when relevant. Everything between the head and the sources panel is chosen per page by the agent from the component library.

### Article page

```
┌─────────────────────────────────────────────────────────────┐
│ News   Tech  Business  Finance  Economy  Science    Search  │  masthead
├─────────────────────────────────────────────────────────────┤
│ Fly title (crimson)                          │ On this page │
│ Headline, up to 3 lines                      │ ──────────── │
│ Standfirst                                   │ More on X    │
│ Byline · Data as of · categories             │ ──────────── │
│                                              │ Use this data│
│ ─Key figure─ ─Key figure─ ─Key─ ─Key─        │              │
│ Source line                                  │              │
│                                              │              │
│ Prose (≤66ch)                                │              │
│ ▬━━━━━━━━━━━━━━━━━━━━━━━ chart rule          │              │
│ Chart title / subtitle / chart / source      │              │
│ … more components chosen by the agent …      │              │
│ ┌ Note (tinted): context in Claude's words ┐ │              │
│ ━━ Sources ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━ │              │
│ Report an error · Markdown · JSON · Method   │              │
├─────────────────────────────────────────────────────────────┤
│ Footer                                                      │
└─────────────────────────────────────────────────────────────┘
```

- Main column: max **720px**. Prose inside it: max **66ch**.
- Rail: **250px**, from 980px up, with a 56px gap. Below 980px the rail's blocks move under the sources panel, in a 2-column grid on tablet.
- Charts may use the full 720px. A chart that needs more (a wide map, a long timeline) may break out to the full container width; nothing else does.
- Vertical rhythm between article blocks: 28px.

### Home page (curated front page)

```
┌─────────────────────────────────────────────────────────────┐
│ masthead                                                    │
│ Wednesday 6 November 2024     All regions  N. America  …    │  date line + region filter
├──────────────────────────────────────────┬──────────────────┤
│ Fly title                                │ Fly title        │
│ LEAD HEADLINE (display)                  │ Headline (m)     │
│ Summary                                  │ figure + spark   │
│ Page type, region, data date             │ ──────────────── │
│ ▬━━━━━━━━━━━━━━━ mini chart + key figure │ Fly / Headline   │
├══════════════════════════════════════════╧══════════════════┤
│ Latest   10:42 Fly — headline · 10:15 Fly — headline · …    │  automatic, updates on every publish
├══════════════════════════════════════════════════════════════┤
│ Economy                                   All economy pages │  section rule (2px)
│ ┌story┐  ┌story┐  ┌story┐  ┌story┐                         │  auto-fit, min 220px
├══════════════════════════════════════════════════════════════┤
│ Business …                                                  │  sections and their order chosen
│ Science …                                                   │  by the curation agent
├─────────────────────────────────────────────────────────────┤
│ footer                                                      │
└─────────────────────────────────────────────────────────────┘
```

- **The home page is curated and dynamic.** A curation agent chooses the lead, the two side stories, which sections appear (3–5), their order, and the 3–4 stories in each, every 2–3 days, through the `curate-home` workflow (Technical Design, "Home page curation"). The page is rendered on the server from the stored curation; nothing is rebuilt.
- **Curated stories use their page's own words.** Fly title, headline, summary and figures come from the page record, so an updated page updates its home slot. The curator only chooses which existing key figure or mini chart a slot shows.
- **Latest list:** an automatic strip of the newest 8–10 pages under the lead block, updated on every publish so the page stays fresh between curations. Each entry: time or date (Fira 13, ink-3), fly title and headline on one line, wrapping on phones. A link to the full "Latest" page ends the strip.
- Lead block: two columns at 1.7fr / 1fr from 820px; stacked below.
- Section rows: `grid-template-columns: repeat(auto-fit, minmax(220px, 1fr))`, 3–4 stories each.
- Each story in a row: fly title, headline, one key figure (with optional sparkline), one-line summary, page type and region.
- Story order in a row is the curator's order of importance, not time.
- **Region filter:** shows the curated stories for that region first, topped up with the region's latest pages to fill the same slots.
- **Gaps:** if a curated page is withdrawn, its slot is filled from the best recent page of the same section; the layout never shows an empty slot.
- The template's slot counts (1 lead, 2 side, 3–5 sections × 3–4 stories, 8–10 latest) are fixed; the curation fills them, it never changes the layout.

### Category and sub-category hubs (`/finance`, `/finance/earnings`)

```
│ Fly: "Finance"   Hub title (headline-xl)                    │
│ One-line description of the hub                             │
│ Sub-categories as text links                                │
│ Region filter                                               │
│ ━━ Latest ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━ │
│ Story list: date column │ fly, headline, summary, figure    │
│ … paginated, 20 per page …                                  │
```

- A dated list, newest first, with the date in a narrow left column (Fira Sans 13.5, tertiary ink).
- Pagination at the bottom: "Newer" and "Older" plus page numbers.

### Entity hubs (`/companies/apple`, `/countries/india`)

- Head: entity name as headline, one-line description, region, sector, official website link.
- A small "Latest figures" strip (up to four key figures from the most recent page).
- Timeline list of all the entity's pages, grouped by year, newest first. Each quarter's page stays frozen; the hub rolls forward.

### Search and latest

- Search: one wide input under the masthead, filters (category, region, page type, date) as a single row of selects below it, results as the hub story list.
- Empty result: say what was searched and offer to clear filters. When the daily search limit is reached, say so plainly and link to the category hubs.

### Static pages (methodology, about, data access, glossary, privacy)

- Single reading column, 720px, prose at 66ch, h2 sections, no rail.
- The glossary is an alphabetical list of terms (term in Fira Sans 600, definition in Literata), with an A–Z jump row.

### Report an error

A short form on its own page: what is wrong (required), where on the page (optional), email (optional). See [components.md](components.md#forms-textfield-textarea-select-checkbox).

## Print

- Hide masthead navigation, the rail, search, the region filter and the footer links.
- Keep the wordmark, head, figures, charts, notes and sources. Print the page URL and retrieval date under the headline.
- Charts print in their light-mode colours on white.
