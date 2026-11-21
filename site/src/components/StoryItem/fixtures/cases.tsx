import { ListingsProvider } from "../../../renderer/data/listings";
import { fixtureListings } from "../../../renderer/fixtures/listings";
import type { FixtureCase } from "../../contract";
import { StoryItem, StoryList } from "../StoryItem";

const withListings = (node: React.ReactNode) => (
  <ListingsProvider listings={fixtureListings()}>{node}</ListingsProvider>
);

export const cases: FixtureCase[] = [
  {
    name: "home: a key figure with its sparkline",
    render: () => withListings(<StoryItem pageId="apple-fy2024-services" figure="dp_aapl_sales_services_fy2024" />),
  },
  {
    name: "home: a mini chart from the page's own chart",
    render: () => withListings(<StoryItem pageId="fed-december-2024" chart="path" />),
  },
  {
    name: "home: a side story (headline-m), figure without a trend",
    render: () => withListings(<StoryItem pageId="test-policy-rate" figure="dp_test_policy_rate" size="m" />),
  },
  {
    name: "home: no figure",
    render: () => withListings(<StoryItem pageId="rates-explainer" />),
  },
  {
    name: "home: a figure the page has no value for, a region the filter does not list",
    render: () => withListings(<StoryItem pageId="renderer-edge-cases" figure="dp_edge_null" />),
  },
  {
    name: "home: a figure on a page with no data, no fly title, a very long headline",
    render: () => withListings(<StoryItem pageId="long-headline" figure="dp_aapl_net_sales_fy2024" />),
  },
  {
    name: "home: an unknown chart ID",
    render: () => withListings(<StoryItem pageId="apple-fy2024-results" chart="notAChart" />),
  },
  {
    name: "home: an unknown page ID renders nothing",
    render: () => withListings(<StoryItem pageId="not-a-page" figure="dp_aapl_net_sales_fy2024" />),
  },
  {
    name: "hub list: date column, hairlines between items",
    render: () =>
      withListings(
        <StoryList
          pageIds={["us-cpi-december-2024", "walking-trial", "long-headline", "not-a-page", "rates-explainer"]}
        />,
      ),
  },
  {
    name: "search results: date column, no figure",
    render: () =>
      withListings(
        <StoryList pageIds={["fed-december-2024", "test-policy-rate", "china-annual-report"]} variant="search" />,
      ),
  },
];
