// Component tester's checks for #11 part 1 (page types and list parts), on top of the builder's
// page-lists.test.tsx: every word a list part prints is the page record's, the fixed words, or a date made from the
// record; slot figures resolve against the slot page's own data even inside another page; data, content and
// formatting edge cases; one key event; schemas and the index budget; server rendering and hydration.

import { readFileSync } from "node:fs";
import { join } from "node:path";
import { screen, within } from "@testing-library/react";
import { JSDOM } from "jsdom";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import findingSpec from "../../src/components/Finding/spec";
import { LATEST_MAX, LatestList, latestWhen } from "../../src/components/LatestList/LatestList";
import { LeadStory } from "../../src/components/LeadStory/LeadStory";
import { buildComponentIndex, componentSpecs, indexBudgetProblems } from "../../src/components/library";
import { PageContents } from "../../src/components/PageContents/PageContents";
import { Pagination, pageWindow } from "../../src/components/Pagination/Pagination";
import { decisionWords } from "../../src/components/PolicyDecision/PolicyDecision";
import policyDecisionSpec from "../../src/components/PolicyDecision/spec";
import { RelatedPages } from "../../src/components/RelatedPages/RelatedPages";
import { specs } from "../../src/components/registry.generated";
import { allPagesText, SectionHead } from "../../src/components/SectionHead/SectionHead";
import { StoryItem, StoryList } from "../../src/components/StoryItem/StoryItem";
import { homeSlotProps } from "../../src/components/StoryItem/slot";
import timelineSpec from "../../src/components/Timeline/spec";
import { PageDataProvider } from "../../src/renderer/data/context";
import {
  type ListedPage,
  ListingsProvider,
  PAGE_TYPE_LABELS,
  type PageListing,
  PageListings,
  regionLabel,
} from "../../src/renderer/data/listings";
import { formatValue } from "../../src/renderer/data/resolve";
import { renderComponentFixture } from "../../src/renderer/fixtures/component-pages";
import { FIXTURE_NOW, fixtureListedPages, fixtureListings } from "../../src/renderer/fixtures/listings";
import { syntheticResolver } from "../support/data";
import { hydrateAndCollectWarnings } from "../support/hydrate";
import { expectNoAxeViolations, renderComponent } from "../support/render";
import { repoRoot } from "../support/tokens";

const record = (over: Partial<PageListing> & { page_id: string }): ListedPage => ({
  record: {
    path: `/?story=${over.page_id}`,
    title: `Title of ${over.page_id}`,
    fly_title: `Fly of ${over.page_id}`,
    summary: `Summary of ${over.page_id}.`,
    page_type: "policy",
    region: "north-america",
    published_at: "2025-01-15T09:05:00Z",
    ...over,
  },
});
const withListings = (ui: React.ReactNode, listings: PageListings = fixtureListings()) =>
  renderComponent(<ListingsProvider listings={listings}>{ui}</ListingsProvider>);
const listed = (id: string) => {
  const p = fixtureListedPages().find((x) => x.record.page_id === id);
  if (!p) throw new Error(`no fixture listing ${id}`);
  return p;
};
const fixtureDoc = (name: string, fixture: string) =>
  new JSDOM(renderComponentFixture(name, fixture).article).window.document;

// Sentinel words no component, formatter or fixture could produce on its own.
const SENTINEL = record({
  page_id: "sentinel",
  title: "Zqx headline words",
  fly_title: "Zqx fly words",
  summary: "Zqx summary words.",
});

describe("every word a list part prints is the record's, a date from the record, or the part's fixed words", () => {
  const listings = () => new PageListings([SENTINEL]);
  const cases: [string, React.ReactNode, string][] = [
    [
      "LeadStory",
      <LeadStory key="l" pageId="sentinel" />,
      "Zqx fly wordsZqx headline wordsZqx summary words.Policy decision, North America, 15 Jan 2025",
    ],
    [
      "StoryItem home",
      <StoryItem key="h" pageId="sentinel" />,
      "Zqx fly wordsZqx headline wordsZqx summary words.Policy decision, North America",
    ],
    [
      "StoryItem hub",
      <StoryItem key="u" pageId="sentinel" variant="hub" />,
      "15 Jan 2025Zqx fly wordsZqx headline wordsZqx summary words.Policy decision, North America",
    ],
    [
      "StoryList search",
      <StoryList key="s" pageIds={["sentinel"]} variant="search" />,
      "15 Jan 2025Zqx fly wordsZqx headline wordsZqx summary words.Policy decision, North America",
    ],
    [
      "LatestList",
      <LatestList key="t" pageIds={["sentinel"]} now="2025-01-15T18:00:00Z" />,
      "Latest09:05Zqx fly words Zqx headline wordsAll latest pages",
    ],
    [
      "RelatedPages",
      <RelatedPages key="r" entity={{ name: "Zqx Corp", href: "/?e=zqx" }} pageIds={["sentinel"]} />,
      "More on Zqx CorpZqx headline wordsPolicy decision, 15 January 2025All pages on Zqx Corp",
    ],
  ];
  it.each(cases)("%s", async (_name, ui, expected) => {
    const { container } = withListings(ui, listings());
    expect(container.textContent).toBe(expected);
    // Every link goes to the record's own path, or to a hub the template passed in.
    for (const a of container.querySelectorAll("a")) {
      expect(["/?story=sentinel", "/latest/", "/?e=zqx"]).toContain(a.getAttribute("href"));
    }
    await expectNoAxeViolations(container);
  });

  it("a record with no fly title prints no fly title and no empty element for one", () => {
    const { container } = withListings(
      <>
        <LeadStory pageId="bare" />
        <StoryItem pageId="bare" />
        <LatestList pageIds={["bare"]} now={FIXTURE_NOW} />
      </>,
      new PageListings([record({ page_id: "bare", fly_title: null })]),
    );
    expect(container.querySelectorAll(".fly-title, .latest-list__fly")).toHaveLength(0);
    expect(container.querySelector(".latest-list__link")?.textContent).toBe("Title of bare");
  });

  it("an updated record updates every list that shows it (nothing is copied at curation time)", () => {
    const before = withListings(<StoryItem pageId="sentinel" />, new PageListings([SENTINEL]));
    expect(before.container.textContent).toContain("Zqx headline words");
    before.unmount();
    const updated = { ...SENTINEL, record: { ...SENTINEL.record, title: "Zqx corrected headline" } };
    const after = withListings(<StoryItem pageId="sentinel" />, new PageListings([updated]));
    expect(after.container.textContent).toContain("Zqx corrected headline");
    expect(after.container.textContent).not.toContain("Zqx headline words");
  });
});

describe("slot figures come from the slot page's own data", () => {
  it("inside another page's data (the home page's), a slot still resolves against its own page", () => {
    const services = listed("apple-fy2024-services");
    if (!services.data) throw new Error("fixture page needs data");
    const own = services.data.value("dp_aapl_sales_services_fy2024");
    if (!own.ok) throw new Error("fixture value should resolve");
    // The outer provider is the home page's own data, with the same reference holding a different value.
    const home = syntheticResolver({
      points: { dp_aapl_sales_services_fy2024: { value: "1", currency: "USD", scale: "units" } },
    });
    const { container } = withListings(
      <PageDataProvider resolver={home}>
        <StoryItem pageId="apple-fy2024-services" figure="dp_aapl_sales_services_fy2024" />
      </PageDataProvider>,
    );
    expect(container.querySelector(".slot-figure__figure .source-ref")?.textContent).toBe(formatValue(own));
    expect(container.querySelector(".source-ref--missing")).toBeNull();
  });

  it("a key figure's sparkline is its own trend series and is named from the data", () => {
    const { container } = withListings(
      <StoryItem pageId="apple-fy2024-services" figure="dp_aapl_sales_services_fy2024" />,
    );
    const svg = container.querySelector("svg.sparkline");
    expect(svg?.getAttribute("data-ref")).toBe("sr_aapl_services_5y");
    expect(svg?.getAttribute("role")).toBe("img");
    expect(svg?.getAttribute("aria-label")).toMatch(/^Trend, fiscal 2020 to fiscal 2024: \$[\d.]+bn to \$96\.2bn$/);
  });

  it("search results never show a figure, even when given one", () => {
    const { container } = withListings(
      <StoryItem pageId="apple-fy2024-services" figure="dp_aapl_sales_services_fy2024" variant="search" />,
    );
    expect(container.querySelector(".slot-figure, svg")).toBeNull();
  });

  it("the lead story shows its figure at the full figure size, story items at figure-s", () => {
    const { container } = withListings(
      <>
        <LeadStory pageId="fed-december-2024" figure="dp_fed_upper_20241218" />
        <StoryItem pageId="fed-december-2024" figure="dp_fed_upper_20241218" />
      </>,
    );
    const figures = [...container.querySelectorAll(".slot-figure__figure")].map((f) => f.className);
    expect(figures[0]).toContain("text-figure");
    expect(figures[0]).not.toContain("text-figure-s");
    expect(figures[1]).toContain("text-figure-s");
  });

  it("the slot schema rejects text in every field and accepts no field beyond the three", () => {
    for (const bad of [
      { pageId: "apple-fy2024-results", figure: "dp_aapl_net_sales_fy2024", note: "Curator words" },
      { pageId: "apple-fy2024-results", chart: "A chart about Apple" },
      { pageId: "apple-fy2024-results", figure: "sr_aapl_services_5y" },
      { pageId: "-leading-hyphen" },
      { pageId: "" },
    ]) {
      expect(homeSlotProps.safeParse(bad).success, JSON.stringify(bad)).toBe(false);
    }
    expect(homeSlotProps.safeParse({ pageId: "fed-december-2024", chart: "path" }).success).toBe(true);
  });
});

describe("lists: edge cases", () => {
  const many = Array.from({ length: 14 }, (_, i) =>
    record({ page_id: `p${i}`, published_at: `2025-01-${String(15 - i).padStart(2, "0")}T0${i % 10}:00:00Z` }),
  );

  it("a page named twice is listed once (LatestList, StoryList, RelatedPages)", () => {
    const listings = new PageListings(many);
    const { container } = withListings(
      <>
        <LatestList pageIds={["p0", "p1", "p0"]} now={FIXTURE_NOW} />
        <StoryList pageIds={["p2", "p2", "p3"]} />
        <RelatedPages entity={{ name: "X", href: "/?x" }} pageIds={["p4", "p4", "p5"]} />
      </>,
      listings,
    );
    expect(container.querySelectorAll(".latest-list__item")).toHaveLength(2);
    expect(container.querySelectorAll(".story-list__item")).toHaveLength(2);
    expect(container.querySelectorAll(".related-pages__item")).toHaveLength(2);
  });

  it(`LatestList stops at ${LATEST_MAX}, RelatedPages at three, StoryList shows every page`, () => {
    const ids = many.map((p) => p.record.page_id);
    const { container } = withListings(
      <>
        <LatestList pageIds={ids} now={FIXTURE_NOW} />
        <RelatedPages entity={{ name: "X", href: "/?x" }} pageIds={ids} />
        <StoryList pageIds={ids} />
      </>,
      new PageListings(many),
    );
    expect(container.querySelectorAll(".latest-list__item")).toHaveLength(LATEST_MAX);
    expect(container.querySelectorAll(".related-pages__item")).toHaveLength(3);
    expect(container.querySelectorAll(".story-list__item")).toHaveLength(14);
  });

  it("empty and unknown lists render nothing at all: no heading, no landmark, no empty list", () => {
    const { container } = withListings(
      <>
        <LatestList pageIds={[]} now={FIXTURE_NOW} />
        <StoryList pageIds={["nope"]} />
        <RelatedPages entity={{ name: "X", href: "/?x" }} pageIds={[]} />
        <PageContents items={[]} />
        <LeadStory pageId="nope" />
        <StoryItem pageId="nope" />
      </>,
      new PageListings([]),
    );
    expect(container.innerHTML).toBe("");
  });

  it("the listings remember which IDs were missing, so the template can back-fill the slot", () => {
    const listings = new PageListings([SENTINEL]);
    withListings(
      <>
        <LeadStory pageId="gone" />
        <StoryItem pageId="sentinel" />
      </>,
      listings,
    );
    expect([...listings.missing]).toEqual(["gone"]);
  });

  it("list parts outside a ListingsProvider fail loudly, not silently empty", () => {
    expect(() => renderToStaticMarkup(<StoryItem pageId="x" />)).toThrow(/ListingsProvider/);
  });

  it("every page type in the contract has a reader label, and region codes read as place names", () => {
    const common = JSON.parse(readFileSync(join(repoRoot, "contracts/cloud/common.schema.json"), "utf8"));
    expect(Object.keys(PAGE_TYPE_LABELS).sort()).toEqual([...common.$defs.pageType.enum].sort());
    expect(regionLabel("north-america")).toBe("North America");
    expect(regionLabel("latin-america")).toBe("Latin America");
    expect(regionLabel("middle-east-and-north-africa")).toBe("Middle East and North Africa");
    expect(regionLabel("global")).toBe("Global");
  });
});

describe("formatting: times, dates and the Latest column", () => {
  it("times are 24-hour UTC with leading zeros; other days are short dates; offsets count in UTC", () => {
    expect(latestWhen("2025-01-15T00:05:00Z", FIXTURE_NOW)).toBe("00:05");
    expect(latestWhen("2025-01-15T23:59:59Z", FIXTURE_NOW)).toBe("23:59");
    expect(latestWhen("2025-01-15T01:00:00+02:00", FIXTURE_NOW)).toBe("14 Jan 2025");
    expect(latestWhen("2024-09-03T10:00:00Z", FIXTURE_NOW)).toBe("3 Sep 2024");
    expect(latestWhen("2024-01-15T10:00:00Z", FIXTURE_NOW)).toBe("15 Jan 2024");
    expect(latestWhen("2025-01-15T10:00:00.250Z", FIXTURE_NOW)).toBe("10:00");
  });

  it("each list writes its machine-readable date from the record", () => {
    const { container } = withListings(
      <>
        <LatestList pageIds={["sentinel"]} now={FIXTURE_NOW} />
        <StoryItem pageId="sentinel" variant="hub" />
        <LeadStory pageId="sentinel" />
        <RelatedPages entity={{ name: "X", href: "/?x" }} pageIds={["sentinel"]} />
      </>,
      new PageListings([SENTINEL]),
    );
    const times = [...container.querySelectorAll("time")].map((t) => t.getAttribute("datetime"));
    expect(times).toEqual(Array(4).fill("2025-01-15T09:05:00Z"));
  });
});

describe("Pagination", () => {
  it("the window keeps first, last, current and neighbours, in order, never a one-page gap, at most 7 numbers", () => {
    for (let total = 1; total <= 30; total++) {
      for (let current = 1; current <= total; current++) {
        const w = pageWindow(current, total);
        const nums = w.filter((n): n is number => n !== "gap");
        for (const must of [1, total, current, current - 1, current + 1].filter((n) => n >= 1 && n <= total)) {
          expect(nums, `${current}/${total}`).toContain(must);
        }
        for (let i = 1; i < w.length; i++) {
          const [a, b] = [w[i - 1], w[i]];
          if (typeof a === "number" && typeof b === "number") expect(b - a, `${current}/${total}`).toBe(1);
          if (b === "gap") {
            const next = w[i + 1];
            expect(typeof a === "number" && typeof next === "number" && next - a > 2, `${current}/${total}`).toBe(true);
          }
        }
        expect(nums.length).toBeLessThanOrEqual(7);
      }
    }
  });

  it("out-of-range and non-finite input shows page 1, the last page, or nothing; never NaN or Infinity", () => {
    const { container } = renderComponent(
      <>
        <Pagination current={0} total={3} label="a" />
        <Pagination current={99} total={3} label="b" />
        <Pagination current={Number.NaN} total={3} label="c" />
        <Pagination current={1} total={Number.NaN} label="d" />
        <Pagination current={1} total={Number.POSITIVE_INFINITY} label="e" />
        <Pagination current={2.7} total={3.9} label="f" />
        <Pagination current={1} total={0} label="g" />
      </>,
    );
    const current = (label: string) =>
      container.querySelector(`nav[aria-label="${label}"] [aria-current="page"]`)?.textContent ?? null;
    expect(current("a")).toBe("Page 1");
    expect(current("b")).toBe("Page 3");
    expect(current("c")).toBe("Page 1");
    expect(current("f")).toBe("Page 2");
    for (const gone of ["d", "e", "g"]) expect(container.querySelector(`nav[aria-label="${gone}"]`)).toBeNull();
    expect(container.textContent).not.toMatch(/NaN|Infinity/);
  });

  it("links are plain: href per page, rel prev/next, a custom address function, one current page", () => {
    renderComponent(<Pagination current={5} total={9} href={(n) => `/latest/${n}/`} label="Latest pages" />);
    const nav = screen.getByRole("navigation", { name: "Latest pages" });
    const links = within(nav).getAllByRole("link");
    expect(links.map((a) => a.textContent)).toEqual([
      "Newer",
      "Page 1",
      "Page 4",
      "Page 5",
      "Page 6",
      "Page 9",
      "Older",
    ]);
    expect(links.map((a) => a.getAttribute("href"))).toEqual([
      "/latest/4/",
      "/latest/1/",
      "/latest/4/",
      "/latest/5/",
      "/latest/6/",
      "/latest/9/",
      "/latest/6/",
    ]);
    expect(links[0]).toHaveAttribute("rel", "prev");
    expect(links.at(-1)).toHaveAttribute("rel", "next");
    expect(nav.querySelectorAll('[aria-current="page"]')).toHaveLength(1);
    // The ellipses are hidden from screen readers; the page numbers say "Page n".
    expect([...nav.querySelectorAll(".pagination__gap")].every((g) => g.getAttribute("aria-hidden") === "true")).toBe(
      true,
    );
  });
});

describe("SectionHead", () => {
  it("the hub link lower-cases the category name mid-sentence but keeps words in capitals", () => {
    expect(allPagesText("Economy")).toBe("All economy pages");
    expect(allPagesText("Central banks")).toBe("All central banks pages");
    expect(allPagesText("AI and robotics")).toBe("All AI and robotics pages");
    expect(allPagesText("A")).toBe("All a pages");
  });

  it("level 3 heading when nested, the id passed through, and no link without a hub", () => {
    const { container } = renderComponent(<SectionHead name="Science" level={3} id="sec-science" />);
    expect(screen.getByRole("heading", { level: 3, name: "Science" })).toHaveAttribute("id", "sec-science");
    expect(container.querySelector("a")).toBeNull();
  });
});

describe("PageContents", () => {
  it("links each item to its anchor and fills {ref} tokens from the page's data, uncited", async () => {
    const data = syntheticResolver({ periods: { pd_fy2024: { label: "fiscal 2024", fiscal: true } } });
    const { container } = renderComponent(
      <PageDataProvider resolver={data}>
        <PageContents
          items={[
            { id: "why", text: "Why sales rose in {pd_fy2024}" },
            { id: "gone", text: "A title with {dp_not_here}" },
          ]}
        />
      </PageDataProvider>,
    );
    const nav = screen.getByRole("navigation", { name: "On this page" });
    expect(
      within(nav)
        .getAllByRole("link")
        .map((a) => [a.textContent, a.getAttribute("href")]),
    ).toEqual([
      ["Why sales rose in fiscal 2024", "#why"],
      ["A title with no data", "#gone"],
    ]);
    expect(nav.querySelector("button, [popovertarget]")).toBeNull();
    expect(nav.querySelector("ol")).toBeNull(); // no numbering
    await expectNoAxeViolations(container);
  });
});

describe("PolicyDecision", () => {
  const change = (value: string, unit: string, decimals: number | null = null) => {
    const data = syntheticResolver({ points: { dp_c: { value, unit, decimals } } });
    const v = data.value("dp_c");
    if (!v.ok) throw new Error("synthetic value should resolve");
    return decisionWords(v);
  };

  it("change words in the data's own unit, never converted, with the true minus never shown", () => {
    expect(change("-25", "basis points")).toBe("Cut by 25 basis points");
    expect(change("25", "basis points")).toBe("Raised by 25 basis points");
    expect(change("-0.25", "percentage points")).toBe("Cut by 0.25 points");
    expect(change("0.50", "percentage points", 2)).toBe("Raised by 0.50 points");
    expect(change("0", "basis points")).toBe("Unchanged");
    expect(change("-0", "percentage points")).toBe("Unchanged");
    expect(change("0.00", "percentage points", 2)).toBe("Unchanged");
    for (const words of [change("-25", "basis points"), change("-0.25", "percentage points")]) {
      expect(words).not.toMatch(/[−-]\d/);
    }
  });

  it("an unknown fact says No data with a capital, as the rate does; so does the Markdown", () => {
    const result = renderComponentFixture("PolicyDecision", "edge-cases");
    const doc = new JSDOM(result.article).window.document;
    const facts = [...doc.querySelectorAll(".policy-decision__fact-value")].map((d) => d.textContent);
    expect(facts).toContain("No data");
    expect(facts).not.toContain("no data");
    expect(result.files["index.md"]).toContain("- **Unknown fact:** No data");
  });

  it("a quoted stance is a reading-text fact that takes the whole row on phones", () => {
    const doc = fixtureDoc("PolicyDecision", "edge-cases");
    const quoteFacts = doc.querySelectorAll(".policy-decision__fact--q");
    expect(quoteFacts).toHaveLength(1);
    expect(quoteFacts[0]?.querySelector(".policy-decision__fact-value--q")?.textContent).toMatch(/^“.+”$/);
    const css = readFileSync(join(repoRoot, "site/src/components/PolicyDecision/PolicyDecision.css"), "utf8");
    expect(css).toMatch(/\.policy-decision__fact--q\s*\{\s*grid-column:\s*1\s*\/\s*-1;/);
  });

  it("the schema allows three facts, not four, and refuses typed numbers and the wrong kinds of reference", () => {
    const fact = { label: "Vote", value: "cx_fomc_vote_20241218" };
    const base = { label: "Rate", rate: "dp_fed_upper_20241218" };
    expect(policyDecisionSpec.props.safeParse({ ...base, facts: [fact, fact, fact] }).success).toBe(true);
    expect(policyDecisionSpec.props.safeParse({ ...base, facts: [fact, fact, fact, fact] }).success).toBe(false);
    expect(policyDecisionSpec.props.safeParse({ ...base, rate: "4.50" }).success).toBe(false);
    expect(policyDecisionSpec.props.safeParse({ ...base, change: "pd_fomc_20241218" }).success).toBe(false);
    expect(policyDecisionSpec.props.safeParse({ ...base, facts: [{ label: "Vote", value: "11 to 1" }] }).success).toBe(
      false,
    );
    expect(policyDecisionSpec.props.safeParse({ ...base, label: "" }).success).toBe(false);
  });

  it("the change is in ink-2 words, never a colour, and no decision element carries the brand colour", () => {
    const css = readFileSync(join(repoRoot, "site/src/components/PolicyDecision/PolicyDecision.css"), "utf8");
    expect(css).not.toMatch(/--color-brand|--chart-highlight|#[0-9a-f]{3,6}\b/i);
  });
});

describe("Finding", () => {
  it("one source line names each cited source once; the Markdown gives the badge words", () => {
    const result = renderComponentFixture("Finding", "edge-cases");
    const doc = new JSDOM(result.article).window.document;
    const mixed = doc.querySelectorAll(".finding")[2];
    expect(mixed?.querySelectorAll(".finding__source a")).toHaveLength(2);
    expect(result.files["index.md"]).toContain("Source type: Preprint, Peer-reviewed");
    expect(result.files["index.md"]).toContain("Unknown figure: No data");
  });

  it("a finding with no source has no badge and no source line, and never invents one", () => {
    const doc = fixtureDoc("Finding", "edge-cases");
    const bare = doc.querySelectorAll(".finding")[0];
    expect(bare?.querySelector(".badge, .finding__source, .finding__figure, .finding__method")).toBeNull();
  });

  it("the schema refuses a typed number for the figure and an empty sentence", () => {
    expect(findingSpec.props.safeParse({ text: "x", figure: "1,204" }).success).toBe(false);
    expect(findingSpec.props.safeParse({ text: "" }).success).toBe(false);
    expect(findingSpec.props.safeParse({ text: "x", figure: "dv_test_bp_change" }).success).toBe(true);
  });
});

describe("Timeline", () => {
  it("one key event at most: two events on the highlighted date mark only the first", () => {
    const data = syntheticResolver({ periods: { pd_a: { label: "1 March 2024" }, pd_b: { label: "2 March 2024" } } });
    const props = {
      events: [
        { date: "pd_a", text: "First." },
        { date: "pd_b", text: "Second." },
        { date: "pd_b", text: "Same day." },
      ],
      highlight: "pd_b",
    };
    const Timeline = timelineSpec.component as unknown as (p: { props: typeof props }) => React.ReactElement;
    const { container } = renderComponent(
      <PageDataProvider resolver={data}>
        <Timeline props={props} />
      </PageDataProvider>,
    );
    const keys = container.querySelectorAll(".timeline__event--key");
    expect(keys).toHaveLength(1);
    expect(keys[0]?.querySelector(".timeline__text")?.textContent).toBe("Second.");
    expect(container.querySelectorAll(".visually-hidden")).toHaveLength(1);
    const md = timelineSpec.markdown?.({ props, data, children: () => "" } as never) ?? "";
    expect(md.match(/\(key event\)/g)).toHaveLength(1);
  });

  it("a highlight that matches no event marks nothing; dates that do not resolve say No data", () => {
    const doc = fixtureDoc("Timeline", "edge-cases");
    const lists = doc.querySelectorAll("ol.timeline");
    expect(lists[2]?.querySelectorAll(".timeline__event--key")).toHaveLength(0);
    const dates = [...(lists[1]?.querySelectorAll(".timeline__date") ?? [])].map((d) => d.textContent);
    expect(dates).toEqual(["Early 2020s, as the source puts it", "No data", "March 2019 to June 2023", "fiscal 2024"]);
    // A period with no start date gets no machine-readable date.
    expect(lists[1]?.querySelectorAll(".timeline__date")[0]?.querySelector("time")).toBeNull();
  });

  it("the schema takes 1 to 40 events and a pd_ highlight only", () => {
    const ev = { date: "pd_fomc_20241218", text: "x" };
    expect(timelineSpec.props.safeParse({ events: [] }).success).toBe(false);
    expect(timelineSpec.props.safeParse({ events: Array(40).fill(ev) }).success).toBe(true);
    expect(timelineSpec.props.safeParse({ events: Array(41).fill(ev) }).success).toBe(false);
    expect(timelineSpec.props.safeParse({ events: [ev], highlight: "dp_fed_upper_20241218" }).success).toBe(false);
    expect(timelineSpec.props.safeParse({ events: [{ date: "18 December 2024", text: "x" }] }).success).toBe(false);
  });
});

describe("index (ADR 0001)", () => {
  it("the three page types are in the index within budget; the template parts are not", () => {
    const index = buildComponentIndex(componentSpecs(specs));
    expect(indexBudgetProblems(index)).toEqual([]);
    const names = index.components.map((c) => c.name);
    for (const n of ["PolicyDecision", "Finding", "Timeline"]) expect(names).toContain(n);
    for (const n of [
      "LeadStory",
      "StoryItem",
      "SectionHead",
      "LatestList",
      "PageContents",
      "RelatedPages",
      "Pagination",
    ]) {
      expect(names).not.toContain(n);
    }
    for (const c of index.components.filter((x) => ["PolicyDecision", "Finding", "Timeline"].includes(x.name))) {
      expect(c.tokens, c.line).toBeLessThanOrEqual(index.budget.perEntry);
    }
  });
});

describe("server rendering", () => {
  it("list parts render complete, script-free HTML and hydrate with no warnings", async () => {
    const ui = (
      <ListingsProvider listings={fixtureListings()}>
        <LeadStory pageId="apple-fy2024-results" chart="chart" />
        <StoryItem pageId="apple-fy2024-services" figure="dp_aapl_sales_services_fy2024" />
        <StoryList pageIds={["us-cpi-december-2024", "walking-trial"]} />
        <LatestList pageIds={["us-cpi-december-2024", "walking-trial"]} now={FIXTURE_NOW} />
        <RelatedPages entity={{ name: "Apple", href: "/?e=apple" }} pageIds={["apple-fy2024-services"]} />
        <PageDataProvider resolver={syntheticResolver({})}>
          <PageContents items={[{ id: "a", text: "A section" }]} />
        </PageDataProvider>
        <SectionHead name="Economy" href="/?s=economy" />
        <Pagination current={2} total={5} />
      </ListingsProvider>
    );
    const { html, warnings } = await hydrateAndCollectWarnings(ui);
    expect(warnings).toEqual([]);
    expect(html).not.toMatch(/<script|data-island|on[a-z]+="/i);
    expect(html).toContain("Services sales grew to $96.2bn in fiscal 2024");
  });

  it("the page types render with no script; only the decision's rate path is an island", () => {
    for (const [name, fixture] of [
      ["Finding", "default"],
      ["Finding", "edge-cases"],
      ["Timeline", "default"],
      ["Timeline", "edge-cases"],
      ["PolicyDecision", "edge-cases"],
    ] as const) {
      expect(renderComponentFixture(name, fixture).article, `${name}/${fixture}`).not.toMatch(/<script|data-island/);
    }
    const islands = renderComponentFixture("PolicyDecision", "default").article.match(/data-island="(\w+)"/g);
    expect(islands).toEqual(['data-island="LineChartPlot"']);
  });
});
