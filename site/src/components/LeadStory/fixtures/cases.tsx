import { ListingsProvider } from "../../../renderer/data/listings";
import { fixtureListings } from "../../../renderer/fixtures/listings";
import type { FixtureCase } from "../../contract";
import { LeadStory } from "../LeadStory";

const withListings = (node: React.ReactNode) => (
  <ListingsProvider listings={fixtureListings()}>{node}</ListingsProvider>
);

export const cases: FixtureCase[] = [
  {
    name: "a mini chart from the page's own chart",
    render: () => withListings(<LeadStory pageId="apple-fy2024-results" chart="chart" />),
  },
  {
    name: "a key figure with its sparkline",
    render: () => withListings(<LeadStory pageId="fed-december-2024" figure="dp_fed_upper_20241218" />),
  },
  { name: "no figure or chart", render: () => withListings(<LeadStory pageId="rates-explainer" />) },
  {
    name: "no fly title, a very long headline, a figure on a page with no data",
    render: () => withListings(<LeadStory pageId="long-headline" figure="dp_aapl_net_sales_fy2024" />),
  },
  {
    name: "a figure the source published no value for",
    render: () => withListings(<LeadStory pageId="renderer-edge-cases" figure="dp_edge_null" />),
  },
  { name: "an unknown page ID renders nothing", render: () => withListings(<LeadStory pageId="not-a-page" />) },
];
