import type { SourceT } from "../../../renderer/bundle/schema";
import type { CitedSource } from "../../../renderer/data/resolve";
import type { FixtureCase } from "../../contract";
import { SourcesPanel } from "../SourcesPanel";

const source = (id: string, s: Omit<SourceT, "connector" | "licence"> & { licence: string }): CitedSource => ({
  id,
  source: { ...s, connector: "fixture", licence: { name: s.licence, redistribution: true } },
});

const tenK = source("src_aapl_10k_2024", {
  title: "Apple Inc. Form 10-K, fiscal 2024",
  short: "Form 10-K, fiscal 2024",
  publisher: "Apple Inc.",
  type: "filing",
  primary: true,
  url: "https://www.sec.gov/Archives/edgar/data/320193/000032019324000123/aapl-20240928.htm",
  licence: "Public filing",
  retrieved_at: "2024-11-01",
});
const release = source("src_aapl_pr_q4", {
  title: "Apple reports fourth quarter results",
  short: "Press release, fourth quarter",
  publisher: "Apple Inc.",
  type: "press_release",
  primary: true,
  url: "https://www.apple.com/newsroom/2024/10/apple-reports-fourth-quarter-results/",
  licence: "Press release",
  retrieved_at: "2024-10-31T21:05:00Z",
});
const outlet = source("src_news_context", {
  title: "Apple's services business keeps growing",
  short: "News article",
  publisher: "An example news outlet",
  type: "news",
  primary: false,
  url: "https://news.example.org/2024/11/apple-services",
  licence: "Linked as context only",
  retrieved_at: "2024-11-01",
});
const paper = source("src_paper", {
  title: "Effect of a daily walking programme on blood pressure: a randomised controlled trial",
  short: "Journal article",
  publisher: "An example medical journal",
  type: "paper",
  primary: true,
  url: "https://doi.org/10.0000/example.2024.001",
  licence: "CC BY 4.0",
  retrieved_at: "2024-11-12",
});
const preprint = source("src_preprint", {
  title: "Walking and blood pressure in older adults: an observational cohort",
  short: "Preprint",
  publisher: "An example preprint server",
  type: "preprint",
  primary: true,
  url: "https://preprints.example.org/2024.11.10.000001",
  licence: "CC BY 4.0",
  retrieved_at: "2024-11-12",
});

// The site's own pages (report form, methodology) are not built yet; fixture links point at the home page. The
// gallery shows several panels, so each takes an id prefix; a real page has one panel and no prefix.
const links = { report: "/?report", markdown: "/?markdown", json: "/?json", methodology: "/?methodology" };

const long = Array.from({ length: 12 }, (_, i) =>
  source(`src_long_${i}`, {
    title: `Statistical release ${String.fromCharCode(65 + i)}: consumer prices, all items, seasonally adjusted, with the full table of components and revisions to earlier months published at https://statistics.example.org/releases/consumer-prices/latest/tables/full`,
    short: `Release ${String.fromCharCode(65 + i)}`,
    publisher: "An example national statistics office with a very long official name",
    type: "dataset",
    primary: i % 4 !== 3,
    url: `https://statistics.example.org/releases/${i}`,
    licence: "Open Government Licence",
    retrieved_at: "2024-11-13",
  }),
);

export const cases: FixtureCase[] = [
  {
    name: "earnings page: primary and context sources, investment disclaimer",
    render: () => (
      <SourcesPanel sources={[outlet, tenK, release]} disclaimer="investment" links={links} idPrefix="earnings-" />
    ),
  },
  {
    name: "updated page: change note",
    render: () => (
      <SourcesPanel
        sources={[tenK]}
        changeNote={{ date: "2024-12-03", text: "Corrected the services share, which had used the wrong year" }}
        disclaimer="investment"
        links={links}
        idPrefix="updated-"
      />
    ),
  },
  {
    name: "science page: peer-reviewed paper and preprint, medical disclaimer",
    render: () => <SourcesPanel sources={[paper, preprint]} disclaimer="medical" links={links} idPrefix="science-" />,
  },
  { name: "no sources", render: () => <SourcesPanel sources={[]} links={links} idPrefix="empty-" /> },
  {
    name: "maximum: twelve sources with long titles and names",
    render: () => <SourcesPanel sources={long} links={links} idPrefix="long-" />,
  },
];
