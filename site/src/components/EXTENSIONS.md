# Extension set

The last step of the component library (#11, Development Plan 3.2 item 12): what the page types still needed to cover "at least what OpenUI Dashboards offers" (Technical Design, "Component library"). Approved by the orchestrator on Karthik's behalf, 28 November 2026. Kept lean: a component is here only when a page type in the PRD needs it and nothing in the library already does the job.

## Built

| Component | Why |
|---|---|
| `SourceCard` | The Dashboards "source card": major report, science and earnings pages rest on one document, and the reader should be able to open it from the body, not only from the sources panel. Every word comes from `sources.json`. |
| `Steps` | Explainers need "diagrams" (PRD page types); numbered steps are the one diagram that reads on a phone, in print and to a screen reader with no drawing. Also the house version of `react-ui`'s `Steps`. |
| `LineChart` `notes` | The Dashboards "time series with annotations" and the explainer's "annotated charts": up to three numbered notes on a line chart, listed under the plot. A prop, not a new chart, so there is one line chart. |

## Already in the library

KPI grids (`KeyFigures`), Sankey, waterfall, segment breakdowns (`SegmentedBar`, `StackedBar`), maps (`ChoroplethMap`), timelines, comparison tables, quotes, image galleries, video embeds and science finding cards (`Finding`) were built in the earlier families, so the Dashboards list is now covered.

## `@openuidev/react-ui` base components not adopted

The page writer gets none of them directly; each is either covered by a house component or breaks a design rule (`design/principles.md`). They also inject runtime CSS (goober), which ADR 0001 says to wrap or rebuild.

| Base component | Why not |
|---|---|
| `Card`, `CardHeader` and the card blocks (`OverviewCardBlock`, `VisualCardBlock`, `SnippetCardBlock` …) | Boxes around content; structure comes from rules and spacing |
| `Tabs`, `Accordion`, `Carousel`, `Modal` | Hide content behind clicks; a page reads top to bottom, prints whole and works without JavaScript |
| `Charts`, `Table`, `EditableTable` | The chart family and `DataTable`/`ComparisonTable` take data references and cite every value |
| `Callout`, `TextCallout` | `Note` is the one tinted block |
| `MetricIndicator` | `KeyFigure` |
| `Image`, `ImageBlock`, `ImageGallery`, `ImageText` | `ImageFigure` and `ImageGallery` (credits, failure fallback) |
| `Tag`, `TagBlock`, `Chips` | `Badge`, chosen from `sources.json` |
| `Separator` | `Rule` |
| `Stack`, `SectionBlock`, `ListBlock`, `EntityList`, `TextContent`, `MarkDownRenderer` | `Page` order and `Prose` (headings, lists) |
| `Steps` | `Steps` (above) |
| `CodeBlock` | No monospace face |
| `Button`, `Buttons`, `IconButton`, `Form` and the inputs, `FollowUpBlock` | Chat controls; page writers place no controls, and the site's controls are page-template parts |
