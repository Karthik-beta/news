import { ListingsProvider } from "../../../renderer/data/listings";
import { fixtureListings } from "../../../renderer/fixtures/listings";
import type { FixtureCase } from "../../contract";
import { RelatedPages } from "../RelatedPages";

const withListings = (node: React.ReactNode) => (
  <ListingsProvider listings={fixtureListings()}>{node}</ListingsProvider>
);
// Entity hubs are not built yet; the link points at the home page with a query.
const apple = { name: "Apple", href: "/?entity=company-apple" };

export const cases: FixtureCase[] = [
  {
    name: "three pages and the hub link (a fourth is left out)",
    render: () =>
      withListings(
        <RelatedPages
          entity={apple}
          pageIds={["apple-fy2024-services", "apple-fy2024-results", "fed-december-2024", "us-cpi-december-2024"]}
        />,
      ),
  },
  {
    name: "one page with a very long headline, an unknown ID left out",
    render: () =>
      withListings(
        <RelatedPages
          entity={{ name: "The Federal Reserve System", href: "/?entity=institution-fed" }}
          pageIds={["not-a-page", "long-headline"]}
        />,
      ),
  },
  {
    name: "no known pages renders nothing",
    render: () => withListings(<RelatedPages entity={apple} pageIds={["not-a-page"]} />),
  },
];
