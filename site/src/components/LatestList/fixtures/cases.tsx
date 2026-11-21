import { ListingsProvider } from "../../../renderer/data/listings";
import { FIXTURE_NOW, fixtureLatestIds, fixtureListings } from "../../../renderer/fixtures/listings";
import type { FixtureCase } from "../../contract";
import { LatestList } from "../LatestList";

const withListings = (node: React.ReactNode) => (
  <ListingsProvider listings={fixtureListings()}>{node}</ListingsProvider>
);
// The full Latest page is not built yet; the link points at the home page with a query.
const allHref = "/?latest";

// A page has one Latest list, so the gallery renders one: a second would repeat the "Latest" landmark name
// (html-validate unique-landmark). The one-page case is covered in tests/component/page-lists.tester.test.tsx.
export const cases: FixtureCase[] = [
  {
    name: "eleven pages and an unknown ID: the newest ten, times for today, dates before",
    render: () =>
      withListings(<LatestList pageIds={["not-a-page", ...fixtureLatestIds()]} now={FIXTURE_NOW} allHref={allHref} />),
  },
  {
    name: "no known pages renders nothing",
    render: () => withListings(<LatestList pageIds={["not-a-page"]} now={FIXTURE_NOW} allHref={allHref} />),
  },
];
