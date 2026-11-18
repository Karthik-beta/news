# Content style

How the site writes: headlines, chart titles, numbers, dates, units and interface text. The agents that write pages follow this; so does every hand-written page.

## Voice

- **Neutral, precise, plain.** Report what the source says. No predictions, recommendations, ratings or opinions.
- **Short sentences, active voice, plain verbs**: rose, fell, grew, held, cut, raised, launched, published.
- **No hype words**: soared, plunged, crashed, skyrocketed, stunning, massive, historic (unless the source itself says "record" and we cite it), game-changing.
- **British spelling** (colour, organisation, programme), but official names keep their own spelling (World Health Organization, Federal Reserve Board, Labor Department).
- **Name things the way readers know them**: "Apple", not "Apple Inc." after the first mention in a sources list; "the Fed" after "the Federal Reserve" once.
- **No first person.** The site never says "we think". Commentary notes are attributed (see [Commentary](#commentary-notes)).

## Headlines

- A full sentence that states the main finding, in the past or present tense: "Services reached a quarter of Apple's sales in fiscal 2024".
- Up to about 80 characters. No full stop. No question marks, no puns, no colons.
- Sentence case. Numbers as figures ("2.4%", "$391.0bn").
- Never a prediction ("will", "set to", "expected to") unless reporting a published forecast, and then name its owner: "The IMF projects global growth of 3.2% in 2024".

## Fly titles (the crimson topic line)

- 2–4 words naming the subject, not the finding: "Apple's annual results", "America's inflation", "India's monetary policy", "Space".
- Sentence case. No full stop. Possessive country names are fine ("China's economy").
- Every article and every home-page story has one.

## Standfirsts and summaries

- Article standfirst: one or two sentences, up to about 200 characters, adding the two or three numbers that support the headline.
- Home-page summary: one sentence, up to about 120 characters.
- Never repeat the headline's wording.

## Chart and table titles

- **Title: the takeaway, as a sentence.** "Services was the only large category to grow by double digits". No full stop.
- **Subtitle: what is plotted, the period and the unit**, as a phrase: "Net sales by reportable segment, US$ millions, as reported".
- The unit appears once, in the subtitle; axis ticks then carry bare numbers. The only exception is a percentage axis, whose top tick may carry `%`.
- A title must be true from the chart alone. If the chart doesn't show it, the title can't say it.

## Numbers

### Exactly as the source states them
- **Currency, units and periods are never converted.** Revenue reported in yen stays in yen; imperial units stay imperial.
- Tables marked "as reported" keep the source's scale and precision ("US$ millions": 167,045).
- Rounded figures in key figures and prose are rounded from the source value by code, never retyped.

### Format

| Kind | Style | Example |
|---|---|---|
| Thousands | Comma separator | 167,045 |
| Decimals | Point | 46.2 |
| Millions, billions, trillions | `m`, `bn`, `trn`, no space after the number | $65.6bn, €1.2trn, 420m |
| Percentages | `%`, no space | 2.4% |
| Change in a percentage | "points" (percentage points); "basis points" for interest rates | Up 2.1 points; cut by 25 basis points |
| Negative numbers | True minus sign `−` (U+2212) | −5.7% |
| Ranges | En dash, no spaces | 2020–2024; 4.50–4.75% |
| Approximation | "About", never "~" | About 3,000 |
| Ordinals | Words up to tenth, then figures | third quarter; 11th |
| Numbers in prose | Figures for 10 and above and for all measurements; words for one to nine when not a measurement | nine central banks; 3 points |

- **Precision**: key figures use one decimal place for bn/m values, unless the source gives fewer. Percentages, points and basis points keep the source's own digits, never rounded: a policy rate of 6.50% is written 6.50%. Never add precision the source doesn't have.
- **Change words**: "Up 2.0% on fiscal 2023", "Down 3.4%", "Unchanged". In compact columns, a signed figure: "+12.9%", "−7.1%". Never coloured red or green.

### Currencies

| Currency | Symbol style | Example |
|---|---|---|
| US dollar | `$` (subtitle says "US$") | $391.0bn |
| Euro, pound, rupee, won | `€`, `£`, `₹`, `₩` | ₹2.1trn |
| Other dollars | Prefix | A$, C$, HK$, NT$, S$ |
| Yen and yuan | `¥` only when the subtitle names the currency; otherwise "yen", "yuan" | ¥1.2trn (Japanese yen) |
| Anything else | ISO code before the number, with a space | CHF 4.5bn |

## Dates and periods

- Full dates: **day month year, no comma**: 1 November 2024. Weekday only on the home page date line.
- Short dates in tables, lists and axes: 1 Nov 2024; months on axes: Jan, Feb … ; years: 2024.
- **Fiscal periods as the company labels them.** Apple: "fiscal 2024", "fourth quarter of fiscal 2024". Microsoft: "first quarter of fiscal 2025". Explain an unusual fiscal year once in a short line ("Microsoft's fiscal year runs from July to June").
- Calendar quarters: "third quarter of 2024" in prose, "Q3 2024" in labels.
- "Data as of" is the latest retrieval date of the page's sources.
- "Updated on 3 December 2024" appears under the byline when a page has been revised, with a one-line reason in the sources panel.

## The fixed lines

| Where | Text |
|---|---|
| Byline | Built with Claude (AI) · Automated checks · Run by Karthik |
| Data date | Data as of 1 November 2024 |
| Finance and economy disclaimer | Not investment advice. |
| Health science disclaimer | Not medical advice. |
| Accuracy note (footer, sources panel) | Built from public sources with AI and checked automatically. It may contain errors; check the linked source before relying on a figure. |
| Licence (footer) | Text and charts are CC BY 4.0. Data keeps its original source licence, shown on each page. |

The byline's middle dots are part of the agreed wording. Don't use middle-dot strings anywhere else; separate metadata with line breaks, commas or spacing.

## Commentary notes

- Anything explanatory in Claude's own words goes in a note block, never in the fact flow.
- The note has a short heading that says what it answers ("Why net income fell while sales rose").
- It ends with an attribution line: "Context written by Claude from the 10-K. Figures are from the filing."
- Notes explain; they never predict, recommend or judge.

## Interface text

- **Buttons and links say what happens**: "Report an error", "Read as Markdown", "Download CSV", "Show the data table", "Load video from YouTube".
- Same name for the same action everywhere. The form button "Send report" leads to "Report sent".
- **Empty states** give direction: "No pages match these filters. Clear the region filter to see all 214 pages."
- **Errors** say what happened and what to do, without apology: "Search is unavailable until 00:00 UTC because today's limit was reached. Browse by category instead."
- No exclamation marks. No "Oops". No emoji.

## Alt text and accessible summaries

- **Images**: describe what is shown and why it is there, in one sentence. Credit and licence go in the visible caption, not the alt text.
- **Charts**: the accessible name states the chart's data in a sentence (all values when there are 12 or fewer; otherwise the range, the endpoints and the extremes). The visible title already gives the takeaway.
- Decorative elements (the chart tag, rules) are hidden from assistive technology.

## Names and sensitive wording

- Use people's full names and roles at first mention: "Jerome Powell, the Fed's chair".
- Describe companies and people factually; attribute every claim to a named source.
- For news-story pages, outlets are linked as context only. Their text is never copied, and a quote from one is limited to a short sentence with attribution.
