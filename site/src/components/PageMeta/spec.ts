import { definePrimitive } from "../contract";

export default definePrimitive({
  name: "PageMeta",
  purpose: "Page-template part: byline, 'Data as of' date, topic links and 'Updated on' line under the standfirst.",
  example: `<PageMeta dataAsOf={latestRetrieval(citedSources)} topics={[{ label: "Earnings", href: "/finance/earnings/" }]}
  updatedOn={page.updatedOn} />  // index.md: pageMetaMarkdown(sameProps)`,
});
