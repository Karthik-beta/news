// Builder's sanity checks for the #11 page types and list components (part 1). Home components show only the page
// record's own text and the page's own data: the rendered fly title, headline, summary and figure equal the
// record's fields, and the slot schema has no field for text. Plus the logic the components lean on: change
// words, page windows, times today, listing parts, honest missing states and the Markdown. Fixture pages and
// cases also pass axe through chrome-text-media.test.tsx and primitives.test.tsx. The formal tests are the
// component tester's (news-component-testing).
import { screen, within } from "@testing-library/react";
import { JSDOM } from "jsdom";
import { describe, expect, it } from "vitest";
import { z } from "zod/v4";
import { LatestList, latestWhen } from "../../src/components/LatestList/LatestList";
import { LeadStory } from "../../src/components/LeadStory/LeadStory";
import { Pagination, pageHref, pageWindow } from "../../src/components/Pagination/Pagination";
import { decisionWords } from "../../src/components/PolicyDecision/PolicyDecision";
import { RelatedPages } from "../../src/components/RelatedPages/RelatedPages";
import { SectionHead } from "../../src/components/SectionHead/SectionHead";
import { StoryItem, StoryList } from "../../src/components/StoryItem/StoryItem";
import { homeSlotProps } from "../../src/components/StoryItem/slot";
import { ListingsProvider, listingParts, PageListings } from "../../src/renderer/data/listings";
import { formatValue } from "../../src/renderer/data/resolve";
import { fixtureBundles } from "../../src/renderer/fixtures/bundles/vite";
import { renderComponentFixture } from "../../src/renderer/fixtures/component-pages";
import { fixtureLibrary } from "../../src/renderer/fixtures/library";
import {
  FIXTURE_NOW,
  fixtureLatestIds,
  fixtureListedPages,
  fixtureListings,
} from "../../src/renderer/fixtures/listings";
import { parsePage } from "../../src/renderer/render";
import { renderComponent } from "../support/render";

const listed = (id: string) => {
  const p = fixtureListedPages().find((x) => x.record.page_id === id);
  if (!p) throw new Error(`no fixture listing ${id}`);
  return p;
};
const withListings = (ui: React.ReactNode, listings = fixtureListings()) =>
  renderComponent(<ListingsProvider listings={listings}>{ui}</ListingsProvider>);
const text = (el: Element | null) => el?.textContent ?? null;

describe("home components show only the page record's own text and data", () => {
  const cases = [
    ["LeadStory", "apple-fy2024-results", { figure: "dp_aapl_net_sales_fy2024" }],
    ["LeadStory", "fed-december-2024", { figure: "dp_fed_upper_20241218" }],
    ["StoryItem", "apple-fy2024-services", { figure: "dp_aapl_sales_services_fy2024" }],
    ["StoryItem", "test-policy-rate", { figure: "dp_test_policy_rate" }],
  ] as const;

  it.each(cases)("%s %s: fly title, headline, summary and figure equal the record's fields", (kind, id, slot) => {
    const page = listed(id);
    const { container } = withListings(
      kind === "LeadStory" ? <LeadStory pageId={id} {...slot} /> : <StoryItem pageId={id} {...slot} />,
    );
    const r = page.record;
    expect(text(container.querySelector(".fly-title"))).toBe(r.fly_title);
    expect(text(container.querySelector(".headline"))).toBe(r.title);
    expect(container.querySelector(".headline a")?.getAttribute("href")).toBe(r.path);
    expect(text(container.querySelector(".lead-story__summary, .story-item__summary"))).toBe(r.summary);
    // The figure is the slot page's own value, through the shared formatter, cited to its own data reference.
    const value = page.data?.value(slot.figure);
    if (!value?.ok) throw new Error(`fixture figure ${slot.figure} should resolve`);
    const figure = container.querySelector(`.slot-figure__figure[data-ref="${slot.figure}"]`);
    expect(text(figure?.querySelector(".source-ref") ?? null)).toBe(formatValue(value));
    // Every character of visible text is the record's, the page's figure and label, or the fixed words.
    const label = container.querySelector(".slot-figure__label");
    expect(text(label)).toBe(page.data?.plainText(page.figures?.[slot.figure]?.label ?? ""));
  });

  it("the slot schema has no field for text: only a page ID and a figure or chart ID, each pattern-checked", () => {
    const json = z.toJSONSchema(homeSlotProps) as { properties: Record<string, { pattern?: string }> };
    expect(Object.keys(json.properties).sort()).toEqual(["chart", "figure", "pageId"]);
    for (const [key, prop] of Object.entries(json.properties)) {
      const pattern = prop.pattern ?? JSON.stringify(prop);
      expect(pattern, key).toMatch(/\^.*\$/);
    }
    expect(
      homeSlotProps.safeParse({ pageId: "apple-fy2024-results", figure: "dp_aapl_net_sales_fy2024" }).success,
    ).toBe(true);
    for (const bad of [
      { pageId: "apple-fy2024-results", headline: "Curator words" },
      { pageId: "apple-fy2024-results", summary: "Curator words" },
      { pageId: "apple-fy2024-results", figure: "Apple made a lot" },
      { pageId: "Apple results!" },
      { pageId: "apple-fy2024-results", figure: "dp_aapl_net_sales_fy2024", chart: "chart" },
    ]) {
      expect(homeSlotProps.safeParse(bad).success, JSON.stringify(bad)).toBe(false);
    }
  });

  it("a record's text is printed exactly as stored, with no {ref} filled in from any page's data", () => {
    const listings = new PageListings([
      {
        record: {
          page_id: "braces",
          path: "/?story=braces",
          title: "A title with {dp_aapl_net_sales_fy2024} in braces",
          fly_title: "Braces {pd_fy2024}",
          summary: "Kept as typed.",
          page_type: "news",
          region: "global",
          published_at: "2025-01-01T00:00:00Z",
        },
      },
    ]);
    withListings(<StoryItem pageId="braces" />, listings);
    expect(screen.getByRole("heading", { level: 3 }).textContent).toBe(
      "A title with {dp_aapl_net_sales_fy2024} in braces",
    );
    expect(screen.getByText("Braces {pd_fy2024}")).toBeInTheDocument();
  });

  it("a mini chart shows the page's own chart title, its latest value and a sparkline of the series", () => {
    const { container } = withListings(<StoryItem pageId="fed-december-2024" chart="path" />);
    const chart = container.querySelector(".slot-figure--chart");
    expect(text(chart?.querySelector(".slot-figure__label") ?? null)).toBe(
      "The Fed cut for the third meeting in a row on 18 December 2024",
    );
    expect(chart?.querySelector(".slot-figure__figure")?.getAttribute("data-ref")).toBe("dp_fed_upper_20241218");
    expect(chart?.querySelector("svg.sparkline")?.getAttribute("data-ref")).toBe("sr_fed_funds_upper");
    expect(text(chart?.querySelector(".slot-figure__period") ?? null)).toBe("18 December 2024");
  });

  it("missing data says so: a page with no data, an unpublished value, an unknown chart; an unknown page is left out", () => {
    const { container } = withListings(
      <>
        <StoryItem pageId="long-headline" figure="dp_aapl_net_sales_fy2024" />
        <StoryItem pageId="renderer-edge-cases" figure="dp_edge_null" />
        <StoryItem pageId="apple-fy2024-results" chart="notAChart" />
        <StoryItem pageId="not-a-page" />
      </>,
    );
    expect(container.querySelectorAll(".story-item")).toHaveLength(3);
    const missing = [...container.querySelectorAll(".slot-figure .source-ref--missing")].map((e) => e.textContent);
    expect(missing).toEqual(["No data", "No data", "No data"]);
    for (const f of container.querySelectorAll(".slot-figure")) expect(f.textContent).not.toMatch(/\b0\b/);
  });

  it("hub and search lists date each item; search shows no figure; the meta line is type and region", () => {
    const { container } = withListings(
      <>
        <StoryList pageIds={["us-cpi-december-2024", "not-a-page", "renderer-edge-cases"]} />
        <StoryList pageIds={["fed-december-2024"]} variant="search" />
      </>,
    );
    const lists = container.querySelectorAll("ol.story-list");
    expect(lists[0]?.querySelectorAll("li")).toHaveLength(2);
    expect([...container.querySelectorAll(".story-item__date time")].map((t) => t.textContent)).toEqual([
      "15 Jan 2025",
      "15 Jan 2025",
      "18 Dec 2024",
    ]);
    expect([...container.querySelectorAll(".story-item__meta")].map((m) => m.textContent)).toEqual([
      "Economic data, North America",
      "Report, Latin America",
      "Policy decision, North America",
    ]);
    expect(lists[1]?.querySelector(".slot-figure")).toBeNull();
  });
});

describe("LatestList", () => {
  it("shows the newest ten: times for pages published today, short dates before", () => {
    const { container } = withListings(<LatestList pageIds={fixtureLatestIds()} now={FIXTURE_NOW} />);
    const items = container.querySelectorAll(".latest-list__item");
    expect(fixtureLatestIds().length).toBeGreaterThan(10);
    expect(items).toHaveLength(10);
    expect([...items].slice(0, 4).map((li) => li.querySelector("time")?.textContent)).toEqual([
      "13:45",
      "09:05",
      "14 Jan 2025",
      "13 Jan 2025",
    ]);
    expect(within(container as HTMLElement).getByRole("region", { name: "Latest" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "All latest pages" })).toHaveAttribute("href", "/latest/");
  });

  it("latestWhen compares UTC calendar dates", () => {
    expect(latestWhen("2025-01-15T00:00:00Z", "2025-01-15T23:59:00Z")).toBe("00:00");
    expect(latestWhen("2025-01-14T23:59:00Z", "2025-01-15T00:00:00Z")).toBe("14 Jan 2025");
  });
});

describe("navigation parts", () => {
  it("SectionHead writes its hub link from the category name", () => {
    renderComponent(<SectionHead name="Economy" href="/economy/" />);
    expect(screen.getByRole("heading", { level: 2, name: "Economy" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "All economy pages" })).toHaveAttribute("href", "/economy/");
  });

  it("RelatedPages shows at most three pages with type and full date, then the hub link", () => {
    withListings(
      <RelatedPages
        entity={{ name: "Apple", href: "/companies/apple" }}
        pageIds={["apple-fy2024-services", "apple-fy2024-results", "fed-december-2024", "us-cpi-december-2024"]}
      />,
    );
    const region = screen.getByRole("region", { name: "More on Apple" });
    expect(within(region).getAllByRole("listitem")).toHaveLength(3);
    expect([...region.querySelectorAll(".related-pages__meta")].map((m) => m.textContent)).toEqual([
      "Earnings, 1 November 2024",
      "Earnings, 1 November 2024",
      "Policy decision, 18 December 2024",
    ]);
    expect(screen.getByRole("link", { name: "All pages on Apple" })).toHaveAttribute("href", "/companies/apple");
  });

  it("pageWindow keeps the ends and the current page's neighbours, never a one-page gap", () => {
    expect(pageWindow(1, 3)).toEqual([1, 2, 3]);
    expect(pageWindow(9, 24)).toEqual([1, "gap", 8, 9, 10, "gap", 24]);
    expect(pageWindow(4, 12)).toEqual([1, 2, 3, 4, 5, "gap", 12]);
    expect(pageWindow(24, 24)).toEqual([1, "gap", 23, 24]);
    expect(pageWindow(1, 1)).toEqual([1]);
    expect(pageHref("/finance/", 1)).toBe("/finance/");
    expect(pageHref("/finance/", 3)).toBe("/finance/?page=3");
    expect(pageHref("/search?q=rates", 2)).toBe("/search?q=rates&page=2");
  });

  it("Pagination marks the current page, links Newer and Older, and shows nothing for one page", () => {
    const { container } = renderComponent(
      <>
        <Pagination current={1} total={1} base="/a/" />
        <Pagination current={2} total={3} base="/finance/" />
      </>,
    );
    expect(container.querySelectorAll("nav")).toHaveLength(1);
    expect(screen.getByRole("link", { name: "Newer" })).toHaveAttribute("href", "/finance/");
    expect(screen.getByRole("link", { name: "Older" })).toHaveAttribute("href", "/finance/?page=3");
    const current = container.querySelectorAll('[aria-current="page"]');
    expect([...current].map((a) => a.textContent)).toEqual(["Page 2"]);
  });
});

describe("page types", () => {
  it("decision words: cut, raised or unchanged, in the change's own unit", () => {
    const words = (value: string, unit: string) =>
      decisionWords({
        ok: true,
        ref: "dp_x",
        kind: "dp",
        label: "x",
        value,
        valueText: value,
        unit,
        currency: null,
        scale: null,
        sourceDecimals: null,
        period: null,
        sources: [],
        derived: null,
        locator: null,
      });
    expect(words("-0.25", "percentage points")).toBe("Cut by 0.25 points");
    expect(words("50", "basis points")).toBe("Raised by 50 basis points");
    expect(words("0", "percentage points")).toBe("Unchanged");
  });

  it("PolicyDecision renders the rate, the change in words, the facts and the rate path's own figure", () => {
    const result = renderComponentFixture("PolicyDecision", "default");
    const doc = new JSDOM(result.article).window.document;
    expect(text(doc.querySelector(".policy-decision__rate .source-ref"))).toBe("4.50%");
    expect(text(doc.querySelector(".policy-decision__change"))).toBe("Cut by 0.25 points");
    expect(
      [...doc.querySelectorAll(".policy-decision__fact-value")].map(
        (d) => (d.querySelector("button") ?? d).textContent,
      ),
    ).toEqual(["11 to 1", "4.75%", "29 January 2025"]);
    expect(doc.querySelector(".policy-decision__path #path-title")).not.toBeNull();
    expect(Object.keys(result.files)).toContain("charts/path.csv");
    expect(result.files["index.md"]).toContain("- **Federal funds rate, upper limit:** 4.50%, cut by 0.25 points");
  });

  it("PolicyDecision and Finding say No data for what is missing, never zero", () => {
    for (const name of ["PolicyDecision", "Finding"]) {
      const doc = new JSDOM(renderComponentFixture(name, "edge-cases").article).window.document;
      expect(doc.querySelectorAll(".source-ref--missing").length, name).toBeGreaterThan(0);
    }
  });

  it("Finding takes its badge from the cited sources' types, never from the writer's words", () => {
    const doc = new JSDOM(renderComponentFixture("Finding", "edge-cases").article).window.document;
    const badges = [...doc.querySelectorAll(".finding")].map((f) =>
      [...f.querySelectorAll(".badge")].map((b) => b.textContent),
    );
    expect(badges).toEqual([[], ["Preprint"], ["Preprint", "Peer-reviewed"], [], ["Peer-reviewed"]]);
  });

  it("Timeline marks one key event in words and colour, and dates events with their period labels", () => {
    const doc = new JSDOM(renderComponentFixture("Timeline", "default").article).window.document;
    const events = doc.querySelectorAll(".timeline__event");
    expect(events).toHaveLength(5);
    expect(doc.querySelectorAll(".timeline__event--key")).toHaveLength(1);
    expect(doc.querySelector(".timeline__event--key .visually-hidden")?.textContent).toBe("Key event: ");
    expect(doc.querySelector(".timeline__event time")?.getAttribute("datetime")).toBe("2022-03-16");
  });
});

describe("listingParts", () => {
  it("finds a page's key figures (label and trend) and charts (title and series) in its page.oui", () => {
    const parts = listingParts(parsePage(fixtureBundles.charts.oui, fixtureLibrary().library));
    expect(parts.figures?.dp_aapl_sales_services_fy2024).toEqual({
      label: "Services net sales",
      trend: "sr_aapl_services_5y",
    });
    expect(parts.charts?.services).toEqual({
      title: "Services sales have grown every year since {pd_fy2020}",
      series: "sr_aapl_services_5y",
    });
    expect(parts.charts?.path?.series).toBe("sr_fed_funds_upper");
  });
});
