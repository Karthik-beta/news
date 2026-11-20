# Components

Every component and primitive lives in its own folder here. The OpenUI library (`createLibrary`) and `component-index.json` are generated from the folders, so nothing is listed by hand. Build components with the `news-component-builder` skill, to the spec in [`design/components.md`](../../../design/components.md) or [`design/charts.md`](../../../design/charts.md). A different Opus agent tests them with `news-component-testing`.

## Folder contract

```
src/components/<Name>/
├─ <Name>.tsx          the component: server-rendered React, written against OpenUI's ComponentRenderer contract
├─ <Name>.client.tsx   only for an interactive part: the island component, default export, plain JSON props
├─ <Name>.css          its styles, tokens only (the token lint fails on a literal colour, font name or px font size)
├─ spec.ts             default-exports defineNewsComponent, defineRootComponent or definePrimitive
├─ fixtures/           components: page.oui files (with data from the fixture bundles); primitives: cases.tsx
└─ SIGNOFF.md          the tester's sign-off (a stub saying "awaiting tester" until then)
```

`bun scripts/gen-components.ts` (run before dev, build and test) checks every folder against this contract and fails with the problems. It also generates:

- `registry.generated.ts`: every `spec.ts`, which `library.ts` turns into the OpenUI library and `component-index.json`;
- `islands.generated.ts`: every `<Name>.client.tsx`, loaded on demand by the island hydrator;
- `../styles/components.generated.css`: every `<Name>.css`, bundled into the site stylesheet (`styles/site.css`).

Commit the generated files. A unit test fails while they are out of date.

## spec.ts

A component's spec (`defineNewsComponent`, see `contract.ts`):

| Field | What |
|---|---|
| `name` | PascalCase, the folder name, the name the page writer calls in OpenUI Lang |
| `index` | The one-line entry: `Name — purpose — data shape`. At most 24 tokens (ADR 0001, counted as `ceil(UTF-8 bytes / 3)`). The whole index stays under 4,000 tokens. `bun run build` writes `component-index.json` and fails if it's over |
| `description` | One sentence for OpenUI's prompt and `news component <type>` |
| `props` | A zod object. Props that take data take references: `z.string().regex(refPattern("sr"))` |
| `dataShape` | Which references each prop takes, in words |
| `example` | One realistic example in OpenUI Lang, references only, no digits in text |
| `component` | The renderer: `({ props, renderNode, statementId }) => ReactNode` |
| `markdown()` | Writes the component for `index.md`. Without it, the component's child elements are written out in order |
| `csv()` | For charts and tables: the download's columns and rows, as exact values and verbatim labels, plus the source IDs |

`Page` is the only root (`defineRootComponent`). Its `children` prop accepts every other component, so its schema is built with the library.

A primitive's spec (`definePrimitive`) has `name`, `purpose` and a TSX `example`. Primitives are building blocks for components, not part of the page writer's library or index.

**Page-template parts are primitives too.** The site chrome (`SkipLink`, `Masthead`, `DateLine`, `RegionFilter`, `Footer`), the article head (`FlyTitle`, `Headline`, `Standfirst`, `PageMeta`), the sources (`SourcesPanel`, `Badge`, `Disclaimer`) and the inline `GlossaryTerm` are rendered by page templates from the page record, `head.json` and `sources.json`, never placed by the page writer, so they stay out of the library and the index. Their purpose says "Page-template part". Where `index.md` needs them, they export a Markdown helper (`pageMetaMarkdown`, `sourcesPanelMarkdown`) for the template to call.

## Page copy

`Prose`, `Note` and `MethodSummary` take the writer's copy with a small markup (`Prose/copy.tsx`):

- one string per block; `## ` starts a section heading (h2, with an id from its words); lines that all start with `- ` are a list; a blank line splits a string into more blocks;
- `{ref}` tokens for every number, period, quote and context line (RefText);
- `[words](glossary:slug)` for a glossary term. The definition comes from the site glossary the renderer is given (`renderPage(…, { glossary })`); a slug the glossary doesn't define renders as plain words.

Nothing else is markup: other brackets, asterisks and `#` print as typed.

## Rules every component follows

1. **Data references, not numbers.** Read data with `usePageData()` (`src/renderer/data/context.tsx`). Write values with `SourceRef` or `formatValue`, and text with `RefText`. The contract is in [`src/renderer/README.md`](../renderer/README.md).
2. **Missing data renders honestly**: "No data", a gap, or the component's documented empty state. Never a zero.
3. **Server-rendered first.** Static components ship no JavaScript. Only the interactive part is an island: render it with `<Island name component props>` from `src/renderer/islands/Island.tsx`. Its props are plain JSON. Don't call OpenUI hooks that need the runtime (state, queries, actions).
4. **Tokens only.** No inline styles except computed geometry in islands (`left`, `top`).
5. **Accessible by construction**: semantic elements, accessible names, keyboard support, visible focus, nothing carried by colour alone.
6. **Charts and tables use `FigureFrame`.**
7. **Class names**: kebab-case of the component (`figure-frame`), elements with `__` (`figure-frame__title`), modifiers with `--` (`rule--brand`).

## Primitives

| Primitive | Use |
|---|---|
| `FigureFrame` | The chart and table frame: brand rule and tag, title, subtitle, legend, plot, source line, "Show the data table" (a native `<details>`), CSV and JSON |
| `SourceRef` | A cited figure from a `dp_`/`dv_` reference with the citation popover. `RefText` renders text with `{ref}` tokens |
| `Popover` | `Popover` (the native `popover` attribute: opens on click, tap, Enter or Space, closes on Escape or a tap outside, 0 KB JS) and `Tooltip` (the same look for islands that position it) |
| `Text` | The type scale as classes (`text-body`, `text-fly` …) and a `<Text variant>` wrapper |
| `Rule` | Hairline, ink, section and brand rules, as an element or as `ruleTop(kind)` classes |
| `VisuallyHidden` | Screen-reader-only text; `focusable` for skip links |
| `Icon` | The eight icons, 1em, in `currentColor`, hidden from assistive technology unless labelled |
| `ChartFigure` | The data-aware chart wrapper: `FigureFrame` filled from one table model (`ChartFigure/model.ts`), so the data table, CSV, JSON, Markdown and accessible summary agree; legend keys; "No data". Also holds the shared chart parts: `scale.ts` (axis, bare labels, x-label thinning), `interaction.tsx` (keys, taps, tooltip, live region, client-safe) and `plot.tsx` |
| `SourcePopover` | The citation popover for one cited value; `SourceRef` renders every cited figure through it |
| `DataLinks` | The downloads row: CSV, JSON and Markdown as text links |
| `Sparkline` | A 120×36 static trend line from an `sr_` series, latest point as a crimson dot |

The shared formatters (numbers, currencies, units, fiscal labels, minus signs, dates) are in `src/format/`.

## Fixture pages

With `NEWS_FIXTURES=1` (test and CI builds), `/fixtures/primitives/<Name>/` shows every case in a primitive's `fixtures/cases.tsx`, `/fixtures/components/<Name>/<fixture>/` renders each component's `fixtures/<fixture>.oui` through the real renderer (data from every fixture bundle merged, including `charts`, plus `src/renderer/fixtures/component-pages/`, the fixture glossary, and test images served at `/fixtures/images/`), with its `index.md` and chart CSV and JSON beside it, and `/fixtures/renderer/<bundle>/` renders each fixture bundle in `src/renderer/fixtures/bundles/`. Production builds leave them out. Each component fixture's `index.md` is snapshotted in `fixtures/__snapshots__/<fixture>.md`.

Links in fixtures must resolve in the fixture build (the integration link check), so fixtures for parts that link to pages not built yet (hubs, methodology, the report form) pass links to the home page with a query string.
