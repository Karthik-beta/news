import { definePrimitive } from "../contract";

export default definePrimitive({
  name: "SourcesPanel",
  purpose:
    "Page-template part: cited sources (primary, then context) with badges, actions row, change note, accuracy note, disclaimer.",
  example: `<SourcesPanel sources={[...resolver.usedSources].map((id) => ({ id, source: resolver.source(id) }))}
  changeNote={page.changeNote} disclaimer="investment" />  // index.md: sourcesPanelMarkdown(sameProps)`,
});
