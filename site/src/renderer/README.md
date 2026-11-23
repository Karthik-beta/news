# Renderer

Turns a page bundle into the files readers and machines get: `index.html`, `index.md`, and a CSV and JSON file per chart. It runs in plain Bun or Node, with no Astro or Vite, so the page runner and the warm render server (#17) call it directly. The approach is [ADR 0001](../../../docs/adr/0001-openui-server-rendering.md), decision B.

```ts
import { renderPage } from "./render";

const result = renderPage(
  { oui, data, sources, head: { flyTitle, headline, summary } },
  { assets: { stylesheets: ["/_astro/site.css"], script: "/_astro/islands.js" } },
);
// result.files: { "index.html", "index.md", "charts/<statement>.csv", "charts/<statement>.json" }
// result.rendererVersion, result.refsUsed, result.sourcesUsed, result.problems
```

## What it does

1. Validates `data.json` and `sources.json` against the bundle contract (`bundle/schema.ts`, published as [`contracts/bundle/`](../../../contracts/bundle/)) and checks their cross-references: every data point's source and period exist, every series point, derived input and context input exists. A contract failure throws `RenderError`.
2. Parses `page.oui` with OpenUI's parser against the component library, then checks every element's props against its zod schema. OpenUI's parser checks types but not refinements, so a typed number where a reference belongs is caught here. A parse or prop error throws `RenderError` (check C1).
3. Renders the article with `react-dom/server` (`renderToStaticMarkup`) inside OpenUI's `Renderer.Root` and our own root slot. It never uses the default `<Renderer>`, which adds wrapper divs with inline styles. Interactive parts are islands (below).
4. Writes `index.md` and the chart downloads through each component's `markdown()` and `csv()` hooks.
5. Wraps the article in the document: exactly one `<meta name="robots">` from `indexStatus` (`noindex` by default, `index, follow` for `"index"`; the write API rewrites this exact tag when `index_status` changes, ADR 0004), `<meta name="generator">` with the renderer version, the stylesheets, and the island script only when the page has an island.

The fallback, option C in the ADR, is our own walk over the parse tree. Select it with `engine: "walker"`. It produces byte-identical HTML, and a test keeps it that way.

`glossary` is the site glossary for `[words](glossary:slug)` terms in page copy (`GlossaryTerm`); without it, terms render as plain words and no definition is made up.

`strict: true` throws on any data problem (unknown reference, missing value, broken link). Publishing uses strict mode. Fixture pages and previews leave it off, so they can show the missing states.

## The data-reference contract

The page writer never types a number, a date or a period. It writes references, and code fills them in from `data.json` through the shared formatters (`src/format/`). The formatters only format: they never convert a value, a unit, a currency or a fiscal period.

### Reference kinds

Six kinds, all lower case `prefix_body` with `_`-separated parts, matching `^(dp|dv|sr|pd|q|cx)_[a-z0-9]+(_[a-z0-9]+)*$` (News — Bureau Design, §3.8):

| Prefix | What | In `data.json` | Resolves to |
|---|---|---|---|
| `dp_` | A data point exactly as the source published it | `points` | a value with unit, currency, scale, period, source and locator |
| `dv_` | A value calculated by code from data points (formula recorded) | `derived` | a value cited as "Calculated by News from" its inputs' sources |
| `sr_` | A series of data points in time order | `series` | the points, with `null` for a period the source has no value for |
| `pd_` | A period label as the source writes it | `periods` | the label, verbatim ("fiscal 2024", "Q4 FY26") |
| `q_` | A quote, an exact substring of a stored text block | `quotes` | the text, in quotation marks |
| `cx_` | A context line computed by code | `context` | the line ("the lowest in the series") |

Sources are keyed `src_…` in `sources.json`. Data points and quotes point at them. Components never take a source ID from the page writer.

### Where references go

- **Component props** take a whole reference as a string: `FixtureFigure("Net sales", "dp_aapl_net_sales_fy2024")`. Each prop's zod schema says which kinds it accepts (`z.string().regex(refPattern("sr"))`). A bare identifier (`dp_x` without quotes) is a parse error, because OpenUI Lang reads it as an undefined statement.
- **Text** (headlines, summaries, fly titles, prose) carries references as `{ref}` tokens: `"Annual inflation in America fell to {dp_7kq2} in {pd_2026_10}"`. Only the braced form is a reference. A reference-shaped word without braces is reported as a `bare-ref` problem. A series can't sit inside text.

### Values

Values in `data.json` are decimal strings exactly as published (`"391035"`, `"-0.3"`), never floats. `value_text` keeps the source's own formatting. `scale` says what the value counts (`"millions"`); the value is not multiplied. `currency` is the ISO 4217 code and is never converted. `decimals` (XBRL `@decimals`) says how precise the source is. The formatters never add precision beyond it.

How a value is written:

| Where | Style | Example |
|---|---|---|
| Key figures, prose, labels | `compact` | `$391.0bn`, `2.4%`, `−5.7%`, `₹2.1trn` |
| Citation popover, as-reported tables | `as-reported` | `$391,035 million` |
| CSV and JSON downloads | the exact decimal | `391035` |

### Missing data

Missing data always renders honestly, never as a zero and never as a crash:

| Case | Problem code | What the reader sees |
|---|---|---|
| The reference is not in `data.json` | `unknown-ref` | "No data" (in a figure), "no data" (inside text) |
| The data point exists but the source published no value (`value: null`) | `no-value` | "No data" |
| A reference of the wrong kind (a series inside text, a period where a value belongs) | `wrong-kind` | "No data" |
| A series point is `null` | none | a gap in the line, an empty CSV field, "No data" in the data table |
| A series, derived value or context line points at a missing reference | `broken-link` | the missing part as above |
| A data point or quote cites a source missing from `sources.json` | `unknown-source` | the value without that source |
| An unknown series in a chart | `unknown-ref` | the chart frame with "No data" and no island |

Every problem is returned in `result.problems`. In strict mode the render fails instead. A page with problems is never published (checks C2 and C3).

### Citations

Every rendered figure carries its reference: a cited figure is a `SourceRef` (a button with a native popover showing the exact value as reported, the document, the location in it, the retrieval date and a link to the source), or a plain `<span class="cited" data-ref="…">` in headlines. The resolver records every reference and source used (`refsUsed`, `sourcesUsed`). The sources list at the end of the article holds exactly the sources the page cited, with `id="source-<src id>"` anchors that chart source lines link to.

### Licences

A chart offers CSV and JSON only when every source behind it allows redistribution (`licence.redistribution`). Otherwise the renderer writes no download files and the chart shows no download links (check C8). The data table is still shown, because citing is always allowed.

## Islands

Interactive parts are islands. Their server side is `islands/Island.tsx`: it renders the component with `renderToString` as its own React root, with its own `useId` prefix, inside `<div data-island="Name" data-props="…">`. The client side is `islands/hydrate.ts`: one shared entry that hydrates each island when it nears the viewport and loads its component on demand. Island props must be plain JSON. The island registry is generated from `src/components/*/<Name>.client.tsx` (`islands.generated.ts`). A page with no island gets no `<script>` at all.

## Renderer version

`version.ts` holds `RENDERER_VERSION`. It is written to the `generator` meta tag, the article's `data-renderer` attribute, `index.md`'s front matter, every chart JSON, and `result.rendererVersion` (for `meta.json`). Bump it when the output for the same bundle changes, so pages can be re-rendered in batches.

## Files

| File | Role |
|---|---|
| `render.tsx` | `renderPage`, `parsePage`, `checkProps`, `renderDocument`, the article template and sources list |
| `bundle/schema.ts` | The zod schemas for `data.json` and `sources.json`; `bun run contracts` writes the JSON Schemas |
| `data/resolve.ts` | `PageDataResolver`: references to values, text segments, problems, `formatValue` |
| `data/context.tsx` | `PageDataProvider` and `usePageData()` for components |
| `islands/` | `Island` (server), `hydrate` and `entry.client.ts` (client) |
| `markdown.ts`, `exports.ts` | `index.md`, chart CSV and JSON, download links and the licence rule |
| `markup.ts` | `staticMarkup`: `renderToStaticMarkup` with the attribute case fixed for `popovertarget` |
| `article.css` | The minimal article template (head, body, sources) until the article components (#7, #9) replace it |
| `fixtures/` | Fixture bundles, fixture-only components (a text block, a key figure, a line chart island), the fixture library and the fixture pages' layout |

## Tests

`src/renderer/render.test.ts` (Vitest, Node), `tests/ssr/` (Bun), `tests/integration/budgets.test.ts` (the built site) and `tests/e2e/renderer/` (Playwright). See [`site/TESTING.md`](../../TESTING.md).
