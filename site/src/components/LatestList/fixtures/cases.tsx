import { ListingsProvider } from "../../../renderer/data/listings";
import { FIXTURE_NOW, fixtureLatestIds, fixtureListings } from "../../../renderer/fixtures/listings";
import type { FixtureCase } from "../../contract";
import { LatestList } from "../LatestList";

const withListings = (node: React.ReactNode) => (
  <ListingsProvider listings={fixtureListings()}>{node}</ListingsProvider>
);
// The full Latest page is not built yet; the link points at the home page with a query.
const allHref = "/?latest";

export const cases: FixtureCase[] = [
  {
    name: "eleven pages: the newest ten, times for today, dates before",
    render: () => withListings(<LatestList pageIds={fixtureLatestIds()} now={FIXTURE_NOW} allHref={allHref} />),
  },
  {
    name: "one page, with an unknown ID left out",
    render: () =>
      withListings(<LatestList pageIds={["not-a-page", "long-headline"]} now={FIXTURE_NOW} allHref={allHref} />),
  },
  {
    name: "no known pages renders nothing",
    render: () => withListings(<LatestList pageIds={["not-a-page"]} now={FIXTURE_NOW} allHref={allHref} />),
  },
];
