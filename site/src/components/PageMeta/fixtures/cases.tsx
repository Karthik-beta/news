import type { FixtureCase } from "../../contract";
import { PageMeta } from "../PageMeta";

// Methodology, the author's site and hubs are not built yet; fixture links point at pages that exist, so the built
// fixture passes the internal link check. The change note lives in the SourcesPanel gallery.
const links = { methodologyHref: "/?methodology", authorHref: "/?author" };
const topics = [
  { label: "Earnings", href: "/?topic=earnings" },
  { label: "Apple", href: "/?topic=apple" },
  { label: "North America", href: "/?topic=north-america" },
];

export const cases: FixtureCase[] = [
  { name: "typical", render: () => <PageMeta dataAsOf="2024-11-01" topics={topics} {...links} /> },
  {
    name: "with a fiscal note and an update",
    render: () => (
      <PageMeta
        dataAsOf="2024-11-01"
        fiscalNote="Apple's fiscal year ends on the last Saturday of September"
        topics={topics}
        updatedOn="2024-12-03"
        changeNoteHref="/fixtures/primitives/SourcesPanel/#updated-change-note"
        {...links}
      />
    ),
  },
  { name: "no topics", render: () => <PageMeta dataAsOf="2024-11-01" {...links} /> },
  { name: "no data date (no sources cited)", render: () => <PageMeta dataAsOf={null} {...links} /> },
  {
    name: "many topics with long names",
    render: () => (
      <PageMeta
        dataAsOf="2026-09-30"
        topics={[
          ...topics,
          { label: "Semiconductors and electronic components", href: "/?topic=semis" },
          { label: "International Business Machines Corporation", href: "/?topic=ibm" },
          { label: "Taiwan Semiconductor Manufacturing Company", href: "/?topic=tsmc" },
          { label: "Global", href: "/?topic=global" },
        ]}
        {...links}
      />
    ),
  },
];
