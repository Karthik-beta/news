// Fixture listings for the list components' galleries and tests (LeadStory, StoryItem, LatestList, RelatedPages):
// page records made the way the publisher makes them. Titles and summaries are the fixture bundles' head copy with
// every {ref} filled in by the resolver (as meta.json stores them), so no figure in a record is typed here; each
// page's data is its own bundle's, and its figures and charts come from its own page.oui (listingParts). Pages
// with a built fixture page link to it; the rest link to the home page with a query, so the link check passes.
import { PageData, PageSources } from "../bundle/schema";
import { type ListedPage, listingParts, type PageListing, PageListings } from "../data/listings";
import { PageDataResolver } from "../data/resolve";
import { parsePage } from "../render";
import { mergeBundleData } from "./bundles/assemble";
import { fixtureBundles } from "./bundles/vite";
import componentData from "./component-pages/data.json";
import componentSources from "./component-pages/sources.json";
import { fixtureLibrary } from "./library";

/** The moment the fixture home page is rendered: pages published on this date show their time. */
export const FIXTURE_NOW = "2025-01-15T16:00:00Z";

const resolverOf = (bundle: { data: unknown; sources: unknown }) =>
  new PageDataResolver(PageData.parse(bundle.data), PageSources.parse(bundle.sources));

const componentPages = {
  oui: "",
  head: { flyTitle: "", headline: "", summary: "" },
  data: componentData,
  sources: componentSources,
};

type Text = Pick<PageListing, "title" | "summary"> & { fly_title?: string | null };

/** A record whose text is head copy with {ref} tokens, filled in from the page's own data. */
function listed(
  data: PageDataResolver | undefined,
  text: Text,
  rest: Omit<PageListing, "title" | "summary" | "fly_title">,
  parts: Pick<ListedPage, "figures" | "charts"> = {},
): ListedPage {
  const fill = (s: string) => (data ? data.plainText(s) : s);
  return {
    record: {
      ...rest,
      title: fill(text.title),
      summary: fill(text.summary),
      fly_title: text.fly_title ? fill(text.fly_title) : null,
    },
    data,
    ...parts,
  };
}

/** A fixture bundle as a listed page: its head, its data, and the figures and charts its page.oui places. */
function fromBundle(name: keyof typeof fixtureBundles, rest: Omit<PageListing, "title" | "summary" | "fly_title">) {
  const bundle = fixtureBundles[name];
  const data = resolverOf(bundle);
  const parts = listingParts(parsePage(bundle.oui, fixtureLibrary().library));
  return listed(
    data,
    { fly_title: bundle.head.flyTitle, title: bundle.head.headline, summary: bundle.head.summary },
    rest,
    parts,
  );
}

export function fixtureListedPages(): ListedPage[] {
  // Pages with no bundle of their own resolve against the charts bundle and the component-page data.
  const extra = resolverOf(mergeBundleData(fixtureBundles.charts, componentPages));
  return [
    fromBundle("apple-fy2024", {
      page_id: "apple-fy2024-results",
      path: "/fixtures/renderer/apple-fy2024/",
      page_type: "earnings",
      region: "north-america",
      published_at: "2024-11-01T10:42:00Z",
    }),
    fromBundle("charts", {
      page_id: "apple-fy2024-services",
      path: "/fixtures/renderer/charts/",
      page_type: "earnings",
      region: "north-america",
      published_at: "2024-11-01T12:15:00Z",
    }),
    listed(
      extra,
      {
        fly_title: "America's interest rates",
        title: "The Fed cut its rate for the third meeting in a row, to {dp_fed_upper_20241218}",
        summary:
          "The Federal Reserve lowered the upper limit of its target range by {dp_fed_change_20241218} on {pd_fomc_20241218}, by a vote of {cx_fomc_vote_20241218}.",
      },
      {
        page_id: "fed-december-2024",
        path: "/?story=fed-december-2024",
        page_type: "policy",
        region: "north-america",
        published_at: "2024-12-18T19:30:00Z",
      },
      {
        figures: { dp_fed_upper_20241218: { label: "Federal funds rate, upper limit", trend: "sr_fed_funds_upper" } },
        charts: {
          path: {
            title: "The Fed cut for the third meeting in a row on {pd_fomc_20241218}",
            series: "sr_fed_funds_upper",
          },
        },
      },
    ),
    listed(
      extra,
      {
        fly_title: "America's inflation",
        title: "America's consumer price index reached {dp_us_cpi_2024_12} in {pd_2024_12}",
        summary: "The index rose in every month of the year, from {dp_us_cpi_2024_01} in {pd_2024_01}.",
      },
      {
        page_id: "us-cpi-december-2024",
        path: "/?story=us-cpi-december-2024",
        page_type: "macro",
        region: "north-america",
        published_at: "2025-01-15T13:45:00Z",
      },
      { figures: { dp_us_cpi_2024_12: { label: "Consumer price index, {pd_2024_12}", trend: "sr_us_cpi_2024" } } },
    ),
    fromBundle("edge-cases", {
      page_id: "renderer-edge-cases",
      path: "/fixtures/renderer/edge-cases/",
      page_type: "report",
      region: "latin-america",
      published_at: "2025-01-15T09:05:00Z",
    }),
    listed(
      extra,
      {
        fly_title: "Exercise and blood pressure",
        title: "Daily walking lowered blood pressure in a trial of {dp_test_trial_participants} adults",
        summary: "Systolic blood pressure after a year differed by {dv_test_bp_change} mmHg between the groups.",
      },
      {
        page_id: "walking-trial",
        path: "/?story=walking-trial",
        page_type: "science",
        region: "global",
        published_at: "2025-01-14T08:00:00Z",
      },
    ),
    listed(
      undefined,
      {
        title:
          "A very long headline with no fly title, written to see how a story item wraps when the page record's title runs over several lines on a narrow phone, with a long word: Pneumonoultramicroscopicsilicovolcanoconiosis",
        summary:
          "A summary just as long as the headline, so that the list's rhythm can be checked when the text is far longer than a normal one-sentence summary and runs on and on across the width of the column.",
      },
      {
        page_id: "long-headline",
        path: "/?story=long-headline",
        page_type: "news",
        region: "global",
        published_at: "2025-01-13T23:59:00Z",
      },
    ),
    listed(
      undefined,
      {
        fly_title: "Monetary policy",
        title: "How central banks set interest rates",
        summary: "What a policy rate is and how a decision reaches the rates people pay.",
      },
      {
        page_id: "rates-explainer",
        path: "/?story=rates-explainer",
        page_type: "explainer",
        region: "europe",
        published_at: "2025-01-12T07:30:00Z",
      },
    ),
    listed(
      extra,
      {
        fly_title: "A test central bank",
        title: "The test central bank raised its policy rate to {dp_test_policy_rate}",
        summary: "The rate rose by {dp_test_policy_change_bp} at the meeting on {pd_test_policy_meeting}.",
      },
      {
        page_id: "test-policy-rate",
        path: "/?story=test-policy-rate",
        page_type: "policy",
        region: "india",
        published_at: "2025-01-10T05:00:00Z",
      },
      { figures: { dp_test_policy_rate: { label: "Policy repo rate" } } },
    ),
    listed(
      undefined,
      {
        fly_title: "China's statistics",
        title: "China's statistics office published its annual report",
        summary: "The report covers output, prices and trade for the year.",
      },
      {
        page_id: "china-annual-report",
        path: "/?story=china-annual-report",
        page_type: "report",
        region: "china",
        published_at: "2025-01-09T02:00:00Z",
      },
    ),
    listed(
      undefined,
      {
        fly_title: "Australia's labour market",
        title: "Australia's jobs report, as the statistics bureau released it",
        summary: "Employment and unemployment for the month, as the bureau states them.",
      },
      {
        page_id: "australia-jobs",
        path: "/?story=australia-jobs",
        page_type: "macro",
        region: "australia",
        published_at: "2025-01-08T00:30:00Z",
      },
    ),
  ];
}

let cached: PageListings | undefined;
/** The fixture listings. A new set each call would reset `missing`; the galleries share one. */
export function fixtureListings(): PageListings {
  cached ??= new PageListings(fixtureListedPages());
  return cached;
}

/** Page IDs, newest first: the fixture "HUB#latest". */
export const fixtureLatestIds = (): string[] =>
  fixtureListedPages()
    .map((p) => p.record)
    .sort((a, b) => (a.published_at < b.published_at ? 1 : -1))
    .map((r) => r.page_id);
