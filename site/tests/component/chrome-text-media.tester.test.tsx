// Tester's rendering, data, formatting and hydration tests for the #7 family (news-component-testing). The builder's
// sanity checks are in chrome-text-media.test.tsx and tests/unit/chrome-text-media.test.ts.
import { fireEvent, render, screen } from "@testing-library/react";
import type React from "react";
import { describe, expect, it } from "vitest";
import { BADGES, Badge, typeBadge } from "../../src/components/Badge/Badge";
import { DateLine } from "../../src/components/DateLine/DateLine";
import { Disclaimer } from "../../src/components/Disclaimer/Disclaimer";
import { FlyTitle } from "../../src/components/FlyTitle/FlyTitle";
import { Footer } from "../../src/components/Footer/Footer";
import { GlossaryProvider, GlossaryTerm } from "../../src/components/GlossaryTerm/GlossaryTerm";
import { Headline } from "../../src/components/Headline/Headline";
import { creditLine } from "../../src/components/ImageFigure/ImageFigure";
import ImageGalleryIsland from "../../src/components/ImageGallery/ImageGallery.client";
import { Masthead } from "../../src/components/Masthead/Masthead";
import { methodRows } from "../../src/components/MethodSummary/MethodSummary";
import { noteAttribution } from "../../src/components/Note/Note";
import { PageMeta } from "../../src/components/PageMeta/PageMeta";
import { RegionFilter, regionHref } from "../../src/components/RegionFilter/RegionFilter";
import { SkipLink } from "../../src/components/SkipLink/SkipLink";
import { orderSources, SourcesPanel, sourceBadges } from "../../src/components/SourcesPanel/SourcesPanel";
import { videoMeta, watchText } from "../../src/components/VideoEmbed/VideoEmbed";
import VideoEmbedIsland, { embedUrl } from "../../src/components/VideoEmbed/VideoEmbed.client";
import type { SourceT } from "../../src/renderer/bundle/schema";
import { PageData, PageSources } from "../../src/renderer/bundle/schema";
import { PageDataProvider } from "../../src/renderer/data/context";
import type { CitedSource } from "../../src/renderer/data/resolve";
import { PageDataResolver } from "../../src/renderer/data/resolve";
import { fixtureBundles } from "../../src/renderer/fixtures/bundles/vite";
import { componentFixtureData } from "../../src/renderer/fixtures/component-pages";
import { fixtureGlossary } from "../../src/renderer/fixtures/glossary";
import { fixtureLibrary } from "../../src/renderer/fixtures/library";
import { staticMarkup } from "../../src/renderer/markup";
import { RenderError, renderPage } from "../../src/renderer/render";
import { hydrateAndCollectWarnings } from "../support/hydrate";

const source = (over: Partial<SourceT> = {}): SourceT =>
  ({
    title: "Form 10-K, fiscal 2024",
    short: "Form 10-K",
    publisher: "Apple Inc.",
    connector: "sec-edgar",
    type: "filing",
    primary: true,
    url: "https://www.sec.gov/",
    licence: { name: "Public filing", redistribution: true },
    retrieved_at: "2024-11-01",
    ...over,
  }) as SourceT;
const cited = (id: string, over: Partial<SourceT> = {}): CitedSource => ({ id, source: source(over) });

/** Parts that fill in {ref} tokens need the page's data, as the template gives them. */
function withData(ui: React.ReactElement) {
  const base = componentFixtureData();
  const resolver = new PageDataResolver(PageData.parse(base.data), PageSources.parse(base.sources));
  return <PageDataProvider resolver={resolver}>{ui}</PageDataProvider>;
}

/** Renders `oui` against the component fixture data plus extra points, with the fixture glossary. */
function renderOui(oui: string, extraPoints: Record<string, unknown> = {}) {
  const base = componentFixtureData();
  const data = structuredClone(base.data) as { points: Record<string, unknown> };
  Object.assign(data.points, extraPoints);
  return renderPage(
    { oui, data, sources: base.sources, head: { flyTitle: "Test", headline: "Test page", summary: "A test page." } },
    { library: fixtureLibrary(), glossary: fixtureGlossary },
  );
}

describe("Formatting through the text components (Karthik: rates keep the source's own digits)", () => {
  const rate = {
    dp_test_repo_rate: {
      label: "Policy repo rate",
      value: "6.50",
      value_text: "6.50",
      unit: "Percent",
      currency: null,
      scale: null,
      period: "pd_edge_2024",
      source: "src_edge_stats",
      locator: { kind: "json", path: "$.rate", series: "REPO" },
    },
  };
  it.each([
    ["Prose", 'p = Prose(["The repo rate stayed at {dp_test_repo_rate}."])'],
    ["Note", 'p = Note("What the bank decided", ["The repo rate stayed at {dp_test_repo_rate}."])'],
    ["MethodSummary", 'p = MethodSummary(null, null, null, "At {dp_test_repo_rate}")'],
  ])("%s prints 6.50%% with the trailing zero, cited", (_name, line) => {
    const result = renderOui(`root = Page([p])\n${line}`, rate);
    expect(result.problems).toEqual([]);
    expect(result.article).toMatch(/>6\.50%</);
    expect(result.article).not.toMatch(/>6\.5%</);
    expect(result.sourcesUsed).toContain("src_edge_stats");
  });

  it("a negative value in prose takes the true minus, never a hyphen", () => {
    const { article } = renderOui('root = Page([p])\np = Prose(["It moved {dp_edge_negative}."])');
    expect(article).toMatch(/−/);
    expect(article).not.toMatch(/>-\d/);
  });

  it("dates: DateLine has the weekday, PageMeta and SourcesPanel do not; datetime is lower case in static HTML", () => {
    const html = staticMarkup(
      <>
        <DateLine date="2024-11-06" />
        <PageMeta dataAsOf="2024-11-01" updatedOn="2024-12-03" />
        <SourcesPanel sources={[cited("a")]} changeNote={{ date: "2024-12-03", text: "Fixed a label" }} />
      </>,
    );
    expect(html).toContain("Wednesday 6 November 2024");
    expect(html).toContain('Data as of <time datetime="2024-11-01">1 November 2024</time>.');
    expect(html).toContain('Updated on <time datetime="2024-12-03">3 December 2024</time>');
    expect(html).toContain("Retrieved 1 November 2024.");
    expect(html).toContain("Updated on 3 December 2024. Fixed a label.");
    expect(html).not.toContain("dateTime=");
  });
});

describe("Rendering every prop of the template parts", () => {
  it("SkipLink targets the given id", () => {
    render(<SkipLink target="content" />);
    expect(screen.getByRole("link", { name: "Skip to content" })).toHaveAttribute("href", "#content");
  });

  it("Masthead: custom sections, home and search links, no current section by default", () => {
    render(<Masthead sections={[{ label: "Tech", href: "/tech/" }]} homeHref="/home/" searchHref="/find/" />);
    expect(screen.getByRole("link", { name: "News" })).toHaveAttribute("href", "/home/");
    expect(screen.getByRole("link", { name: "Search" })).toHaveAttribute("href", "/find/");
    expect(screen.getByRole("navigation", { name: "Sections" })).toBeInTheDocument();
    expect(document.querySelector("[aria-current]")).toBeNull();
  });

  it("RegionFilter: one URL per region, encoded; all regions is the base", () => {
    expect(regionHref("/tech/", "all")).toBe("/tech/");
    expect(regionHref("/tech/", "north-america")).toBe("/tech/?region=north-america");
    expect(regionHref("/", "a b&c")).toBe("/?region=a%20b%26c");
    render(<RegionFilter selected="nowhere" />);
    expect(screen.getAllByRole("link")).toHaveLength(8);
    expect(document.querySelector("[aria-current]")).toBeNull();
  });

  it("Footer: the five links in order and the three fixed lines", () => {
    render(<Footer />);
    const nav = screen.getByRole("navigation", { name: "About this site" });
    expect([...nav.querySelectorAll("a")].map((a) => a.textContent)).toEqual([
      "How pages are made",
      "Glossary",
      "Data access and MCP",
      "Report an error",
      "Privacy",
    ]);
    expect(screen.getByText(/It may contain errors/)).toBeInTheDocument();
    expect(screen.getByText(/CC BY 4\.0/)).toBeInTheDocument();
  });

  it.each([
    ["xl", 1, "text-headline-xl"],
    ["display", 2, "text-display"],
    ["m", 3, "text-headline-m"],
    ["s", 4, "text-headline-s"],
  ] as const)("Headline %s at level %i", (size, level, cls) => {
    render(withData(<Headline text="Sales rose" size={size} level={level} href="/a/" />));
    const h = screen.getByRole("heading", { level });
    expect(h).toHaveClass(cls);
    expect(h.querySelector("a")).toHaveAttribute("href", "/a/");
  });

  it("FlyTitle with and without a link", () => {
    const { rerender } = render(withData(<FlyTitle text="Apple's annual results" />));
    expect(screen.getByText("Apple's annual results")).toHaveClass("text-fly");
    rerender(withData(<FlyTitle text="Apple's annual results" href="/x/" />));
    expect(screen.getByRole("link", { name: "Apple's annual results" })).toHaveAttribute("href", "/x/");
  });

  it("Disclaimer: both kinds, plain text", () => {
    render(
      <>
        <Disclaimer kind="investment" />
        <Disclaimer kind="medical" />
      </>,
    );
    expect(screen.getByText("Not investment advice.")).toBeInTheDocument();
    expect(screen.getByText("Not medical advice.")).toBeInTheDocument();
  });

  it("PageMeta: no data date, a fiscal note made a sentence, topics as a named list, the update link", () => {
    render(
      <PageMeta
        dataAsOf={null}
        fiscalNote="Microsoft's fiscal year runs from July to June"
        topics={[
          { label: "Earnings", href: "/finance/earnings/" },
          { label: "Microsoft", href: "/companies/microsoft/" },
        ]}
        updatedOn="2024-12-03"
        changeNoteHref="#x-change-note"
      />,
    );
    expect(screen.getByText(/Data date not available\. Microsoft's fiscal year runs from July to June\./)).toBeTruthy();
    expect(screen.getByRole("list", { name: "Topics" }).querySelectorAll("a")).toHaveLength(2);
    expect(screen.getByRole("link", { name: "Updated on 3 December 2024" })).toHaveAttribute("href", "#x-change-note");
    expect(screen.getByRole("link", { name: "Built with Claude (AI) · Automated checks" })).toHaveAttribute(
      "href",
      "/methodology/",
    );
  });

  it("PageMeta: three lines when the page has no topics and no update", () => {
    const { container } = render(<PageMeta dataAsOf="2024-11-01" />);
    expect(container.querySelectorAll(".page-meta__line")).toHaveLength(2);
  });
});

describe("Badges and sources: data", () => {
  it("every badge kind says its word; colour only reinforces", () => {
    for (const kind of Object.keys(BADGES) as (keyof typeof BADGES)[]) {
      const { container, unmount } = render(<Badge kind={kind} />);
      expect(container.textContent).toBe(BADGES[kind]);
      unmount();
    }
  });

  it("type badges come from the source type", () => {
    expect(typeBadge({ type: "paper" })).toBe("peer-reviewed");
    expect(typeBadge({ type: "preprint" })).toBe("preprint");
    expect(typeBadge({ type: "news" })).toBeNull();
    expect(sourceBadges(source({ type: "paper", primary: true }))).toEqual(["primary", "peer-reviewed"]);
    expect(sourceBadges(source({ type: "news", primary: false }))).toEqual(["context"]);
  });

  it("SourcesPanel: primary sources first, then context, each with its id; ids carry the prefix", () => {
    const list = [
      cited("n", { primary: false, type: "news", title: "News story" }),
      cited("f"),
      cited("d", { type: "dataset" }),
    ];
    expect(orderSources(list).map((s) => s.id)).toEqual(["f", "d", "n"]);
    const { container } = render(<SourcesPanel sources={list} idPrefix="x-" disclaimer="medical" />);
    expect([...container.querySelectorAll("li[id]")].map((li) => li.id)).toEqual([
      "x-source-f",
      "x-source-d",
      "x-source-n",
    ]);
    expect(container.querySelector("h2")?.id).toBe("x-sources");
    expect(screen.getByText("Not medical advice.")).toBeInTheDocument();
    expect(screen.getAllByRole("link").map((a) => a.textContent)).toEqual(
      expect.arrayContaining(["Report an error", "Read as Markdown", "Page data (JSON)", "How these pages are made"]),
    );
  });

  it("SourcesPanel: no sources says so; forty sources render forty items", () => {
    const { unmount } = render(<SourcesPanel sources={[]} />);
    expect(screen.getByText("No sources.")).toBeInTheDocument();
    unmount();
    const many = Array.from({ length: 40 }, (_, i) => cited(`s${i}`));
    const { container } = render(<SourcesPanel sources={many} />);
    expect(container.querySelectorAll(".sources-panel__item")).toHaveLength(40);
  });

  it("Note attribution: none, one, two and three sources", () => {
    expect(noteAttribution([])).toBe("Context written by Claude from the sources listed on this page.");
    expect(noteAttribution([cited("a")])).toBe(
      "Context written by Claude from Form 10-K. Figures are from the filing.",
    );
    expect(noteAttribution([cited("a"), cited("b", { short: "FRED" })])).toBe(
      "Context written by Claude from Form 10-K and FRED. Figures are from those sources.",
    );
    expect(noteAttribution([cited("a"), cited("b", { short: "B" }), cited("c", { short: "C" })])).toMatch(
      /from Form 10-K, B and C\./,
    );
  });

  it("MethodSummary drops empty and blank rows and keeps the fixed order", () => {
    expect(methodRows({ period: " 2019 ", sourceType: "Paper", sample: "  " }).map((r) => r.label)).toEqual([
      "Source type",
      "Period",
    ]);
    expect(methodRows({})).toEqual([]);
    expect(renderOui("root = Page([m])\nm = MethodSummary()").article).not.toContain("method-summary");
  });
});

describe("Text components: data and content edge cases", () => {
  it("a glossary term outside the glossary is plain words; inside it, a native popover button", () => {
    const html = staticMarkup(
      <GlossaryProvider glossary={fixtureGlossary}>
        <GlossaryTerm slug="core-inflation">core prices</GlossaryTerm> and{" "}
        <GlossaryTerm slug="unknown">neutral rate</GlossaryTerm>
      </GlossaryProvider>,
    );
    expect(html).toMatch(
      /<button type="button" class="popover-trigger glossary-term" popovertarget="[^"]+">core prices/,
    );
    expect(html).toContain(" and neutral rate");
    expect(html).not.toContain("<script");
  });

  it("without a glossary every term is plain text (no made-up definition)", () => {
    const result = renderPage(
      {
        ...fixtureBundles["apple-fy2024"],
        oui: 'root = Page([p])\np = Prose(["A [gross margin](glossary:gross-margin) line."])',
      },
      { library: fixtureLibrary() },
    );
    expect(result.article).toContain("A gross margin line.");
    expect(result.article).not.toContain("glossary-term");
  });

  // Defect PROSE-1 (fixed): heading ids are unique on the page, with -2, -3 suffixes in document order.
  it("two Prose headings with the same words get distinct ids", () => {
    const { article } = renderOui(
      'root = Page([a, b])\na = Prose(["## Outlook", "One."])\nb = Prose(["## Outlook", "Two."])',
    );
    const ids = [...article.matchAll(/<h2[^>]* id="(section-[^"]+)"/g)].map((m) => m[1]);
    expect(ids.length).toBe(2);
    expect(new Set(ids).size, `heading ids ${ids.join(", ")}`).toBe(2);
  });

  it("duplicate heading ids get -2, -3 in document order, the same on every render", () => {
    const oui =
      'root = Page([a, b])\na = Prose(["## Outlook", "One.", "## Outlook 2", "## Outlook"])\nb = Prose(["## Outlook", "Two."])';
    const ids = (html: string) => [...html.matchAll(/<h2[^>]* id="(section-[^"]+)"/g)].map((m) => m[1]);
    const first = ids(renderOui(oui).article);
    expect(first).toEqual(["section-outlook", "section-outlook-2", "section-outlook-3", "section-outlook-4"]);
    expect(ids(renderOui(oui).article)).toEqual(first);
  });

  it("Quote: unknown reference reads No data, and a typed quote is refused", () => {
    const missing = renderOui('root = Page([q])\nq = Quote("q_nope")');
    expect(missing.article).toContain("No data");
    expect(missing.problems.map((p) => p.ref)).toContain("q_nope");
    expect(() => renderOui('root = Page([q])\nq = Quote("We cut rates.")')).toThrow(RenderError);
  });

  it("very long and very short copy, long words and URLs pass through untouched", () => {
    const long = "x".repeat(400);
    const { article } = renderOui(`root = Page([p])\np = Prose(["${long}", "a"])`);
    expect(article).toContain(long);
    expect(article).toContain("<p>a</p>");
  });
});

describe("Media: hosting (Karthik: copy only when the licence allows re-hosting, else link the source's file)", () => {
  const figure = (src: string) =>
    `root = Page([i])\ni = ImageFigure("${src}", "A crater seen from above.", 1000, 1000, "NASA", "Public domain")`;

  it.each([
    ["re-hosted on our storage", "/media/nasa/crater.jpg"],
    ["linked to the source's file", "https://photojournal.jpl.nasa.gov/jpeg/PIA00001.jpg"],
  ])("ImageFigure takes an image %s, unchanged, with no cross-origin or same-origin assumption", (_how, src) => {
    const { article } = renderOui(figure(src));
    expect(article).toContain(`src="${src}"`);
    expect(article).not.toMatch(/crossorigin/i);
  });

  it.each(["http://example.org/a.jpg", "//example.org/a.jpg", "data:image/png;base64,AA", "javascript:x"])(
    "refuses an unsafe image URL: %s",
    (src) => {
      expect(() => renderOui(figure(src))).toThrow(RenderError);
    },
  );

  it("the gallery links each image to its file wherever it is hosted", () => {
    const imgs = ["/media/a.jpg", "https://images.example.org/b.jpg"].map((src) => ({
      src,
      alt: "An image.",
      width: 10,
      height: 10,
      credit: "C",
      licence: "Public domain",
      caption: null,
    }));
    render(<ImageGalleryIsland images={imgs} crop={false} labelledBy={null} />);
    expect(screen.getAllByRole("link").map((a) => a.getAttribute("href"))).toEqual(imgs.map((i) => i.src));
  });

  it("credit lines end each part with one full stop", () => {
    expect(creditLine({ credit: "ESA", licence: "CC BY 4.0." })).toBe("ESA. CC BY 4.0.");
    expect(creditLine({ credit: " NASA ", licence: "Public domain" })).toBe("NASA. Public domain.");
  });

  it("video helpers: meta with and without a date, the link line, the id encoded", () => {
    expect(videoMeta({ publisher: "NASA", publishedOn: null })).toBe("NASA");
    expect(videoMeta({ publisher: "Federal Reserve", publishedOn: "2024-09-18" })).toBe(
      "Federal Reserve, 18 September 2024",
    );
    expect(watchText({ title: " Q4 webcast ", publisher: "Apple" })).toBe("Watch “Q4 webcast” on the Apple site");
    expect(embedUrl("a-b_c")).toBe("https://www.youtube-nocookie.com/embed/a-b_c?autoplay=1");
  });
});

describe("Server rendering: islands hydrate with identical markup and no warnings", () => {
  it("ImageGallery island", async () => {
    const images = [1, 2, 3].map((n) => ({
      src: `/fixtures/images/crater-1000x1000.svg?${n}`,
      alt: `Image ${n}.`,
      width: 1000,
      height: 1000,
      credit: "NASA",
      licence: "Public domain",
      caption: n === 1 ? "A caption." : null,
    }));
    const { warnings } = await hydrateAndCollectWarnings(<ImageGalleryIsland images={images} crop labelledBy="cap" />);
    expect(warnings).toEqual([]);
  });

  it("VideoEmbed island (the link becomes a button after hydration, without a mismatch warning)", async () => {
    const { html, warnings } = await hydrateAndCollectWarnings(
      <VideoEmbedIsland youtubeId="aBcDeFgHiJk" title="FOMC" url="https://www.federalreserve.gov/" />,
    );
    expect(html).toContain('href="https://www.federalreserve.gov/"');
    expect(html).not.toMatch(/youtube-nocookie|<iframe/);
    // The swap from link to button is a deliberate second render, not a hydration mismatch.
    expect(warnings.filter((w) => !w.startsWith("markup changed on hydration"))).toEqual([]);
  });
});

// Re-test of issue #7's fix round (VID-1, IMG-1, PROSE-1), by a tester that did not build the fixes.
describe("Fix round: Prose heading ids (PROSE-1)", () => {
  const ids = (html: string) => [...html.matchAll(/<h2[^>]* id="(section-[^"]+)"/g)].map((m) => m[1]);

  it("three blocks with the same heading: -2 and -3 in document order, every id unique", () => {
    const { article } = renderOui(
      'root = Page([a, b, c])\na = Prose(["## Outlook", "One."])\nb = Prose(["## Outlook", "Two."])\nc = Prose(["## Outlook", "Three."])',
    );
    expect(ids(article)).toEqual(["section-outlook", "section-outlook-2", "section-outlook-3"]);
  });

  it("a suffix never collides with a heading whose own words end in a number", () => {
    const { article } = renderOui('root = Page([a])\na = Prose(["## Outlook", "## Outlook", "## Outlook 2", "x"])');
    const got = ids(article);
    expect(got).toEqual(["section-outlook", "section-outlook-2", "section-outlook-2-2"]);
    expect(new Set(got).size).toBe(got.length);
  });

  it("headings that differ only in punctuation or case share a base and are suffixed", () => {
    const { article } = renderOui('root = Page([a])\na = Prose(["## Outlook!", "## outlook", "x"])');
    expect(ids(article)).toEqual(["section-outlook", "section-outlook-2"]);
  });

  it("an empty heading is section-untitled, and a second one is -2", () => {
    const { article } = renderOui('root = Page([a])\na = Prose(["## ...", "## ???", "x"])');
    expect(ids(article)).toEqual(["section-untitled", "section-untitled-2"]);
  });

  it("each page starts its own count: a second, different page does not inherit the first page's ids", () => {
    renderOui('root = Page([a])\na = Prose(["## Outlook", "## Outlook", "x"])');
    const { article } = renderOui('root = Page([a])\na = Prose(["## Outlook", "x"])');
    expect(ids(article)).toEqual(["section-outlook"]);
  });

  it("the fallback walker gives the same ids as OpenUI's renderer", () => {
    const base = componentFixtureData();
    const input = {
      oui: 'root = Page([a, b])\na = Prose(["## Outlook", "One."])\nb = Prose(["## Outlook", "Two."])',
      data: base.data,
      sources: base.sources,
      head: { flyTitle: "Test", headline: "Test page", summary: "A test page." },
    };
    const opts = { library: fixtureLibrary(), glossary: fixtureGlossary };
    const openui = ids(renderPage(input, opts).article);
    const walker = ids(renderPage(input, { ...opts, engine: "walker" }).article);
    expect(openui).toEqual(["section-outlook", "section-outlook-2"]);
    expect(walker).toEqual(openui);
  });

  it("no id on the page is used twice (Prose headings, Note and the sources panel together)", () => {
    const { article } = renderOui(
      'root = Page([a, n, b])\na = Prose(["## Sources", "One."])\nn = Note("Sources", ["A note."])\nb = Prose(["## Sources", "Two."])',
    );
    const all = [...article.matchAll(/ id="([^"]+)"/g)].map((m) => m[1]);
    expect(all.length).toBeGreaterThan(2);
    expect(all.filter((v, i) => all.indexOf(v) !== i)).toEqual([]);
  });
});

describe("Fix round: VideoEmbed caption under the box (VID-1)", () => {
  const FED =
    'root = Page([v])\nv = VideoEmbed("  FOMC press conference, September 2024 ", "Federal Reserve", "https://www.federalreserve.gov/", "aBcDeFgHiJk", "2024-09-18")';
  const dom = (html: string) => {
    const host = document.createElement("div");
    host.innerHTML = html;
    return host;
  };

  it("title, publisher and date, then the privacy line, in a figcaption that is the figure's last child", () => {
    const host = dom(renderOui(FED).article);
    const figure = host.querySelector("figure.video-embed");
    const caption = figure?.querySelector(":scope > figcaption.video-embed__caption");
    expect(caption).not.toBeNull();
    expect(figure?.lastElementChild).toBe(caption);
    expect([...(caption?.children ?? [])].map((c) => [c.className, c.textContent])).toEqual([
      ["video-embed__title text-headline-s", "FOMC press conference, September 2024"],
      ["video-embed__meta text-label", "Federal Reserve, 18 September 2024"],
      ["video-embed__notice text-source", "Loads from YouTube. YouTube will receive your IP address."],
    ]);
    expect(figure?.getAttribute("aria-label")).toBe("Video: FOMC press conference, September 2024");
  });

  it("the caption is static page markup, outside the island; the 16:9 box holds only the play link", () => {
    const host = dom(renderOui(FED).article);
    const island = host.querySelector('[data-island="VideoEmbed"]');
    expect(island).not.toBeNull();
    expect(island?.querySelector("figcaption, p, .video-embed__title, .video-embed__notice")).toBeNull();
    expect(island?.textContent?.trim()).toBe("");
    const frame = island?.querySelector(".video-embed__frame");
    expect(frame?.children.length).toBe(1);
    expect(frame?.firstElementChild?.getAttribute("aria-label")).toBe(
      "Load video from YouTube: FOMC press conference, September 2024",
    );
    // The island gets only what it needs; the caption's text is not sent twice.
    expect(JSON.parse(island?.getAttribute("data-props") ?? "{}")).toEqual({
      youtubeId: "aBcDeFgHiJk",
      title: "FOMC press conference, September 2024",
      url: "https://www.federalreserve.gov/",
    });
  });

  it("the caption stays after the player loads", async () => {
    const { container } = render(
      <figure className="video-embed">
        <VideoEmbedIsland youtubeId="aBcDeFgHiJk" title="FOMC" url="https://www.federalreserve.gov/" />
        <figcaption className="video-embed__caption">FOMC</figcaption>
      </figure>,
    );
    fireEvent.click(await screen.findByRole("button", { name: "Load video from YouTube: FOMC" }));
    expect(container.querySelector("iframe")).not.toBeNull();
    expect(container.querySelector("figcaption")?.textContent).toBe("FOMC");
  });

  it("without a date the meta is the publisher alone; a video that can't be embedded is still a link line", () => {
    const host = dom(
      renderOui(
        'root = Page([a, b])\na = VideoEmbed("Launch", "NASA", "https://www.nasa.gov/", "Lm_NoPq-RsT")\nb = VideoEmbed("Webcast", "Apple", "https://investor.apple.com/")',
      ).article,
    );
    expect(host.querySelector(".video-embed__meta")?.textContent).toBe("NASA");
    const link = host.querySelector("p.video-embed--link");
    expect(link?.textContent).toContain("Watch “Webcast” on the Apple site");
    expect(link?.querySelector("figcaption")).toBeNull();
    expect(host.querySelectorAll("figcaption.video-embed__caption")).toHaveLength(1);
  });
});

describe("Fix round: image fallback layer (IMG-1)", () => {
  const IMG =
    'root = Page([i])\ni = ImageFigure("/fixtures/images/orbit-1600x900.svg", "Diagram of the orbit.", 1600, 900, "NASA/JPL-Caltech", "Public domain", "The orbit.")';

  it("the frame carries the alt text and credit for its ::before layer, the same as the image's", () => {
    const host = document.createElement("div");
    host.innerHTML = renderOui(IMG).article;
    const frame = host.querySelector(".image-frame");
    const img = frame?.querySelector("img");
    expect(frame?.getAttribute("data-alt")).toBe("Diagram of the orbit.");
    expect(frame?.getAttribute("data-alt")).toBe(img?.getAttribute("alt"));
    expect(frame?.getAttribute("data-credit")).toBe("NASA/JPL-Caltech. Public domain.");
    expect(frame?.getAttribute("data-credit")).toBe(img?.getAttribute("data-credit"));
    // The layer's text is not in the accessibility tree twice: the frame has no role or label of its own.
    expect(frame?.getAttribute("role")).toBeNull();
    expect(frame?.getAttribute("aria-label")).toBeNull();
  });

  it("the gallery's frames carry the same layer data, escaped in the attribute", async () => {
    const images = [
      { src: "/a.svg", alt: 'Alt with "quotes" & <angle>.', width: 10, height: 10, credit: "A", licence: "B" },
    ];
    const { html } = await hydrateAndCollectWarnings(
      <ImageGalleryIsland images={images} crop={false} labelledBy="c" />,
    );
    const host = document.createElement("div");
    host.innerHTML = html;
    expect(host.querySelector(".image-frame")?.getAttribute("data-alt")).toBe('Alt with "quotes" & <angle>.');
    expect(host.querySelector(".image-frame")?.getAttribute("data-credit")).toBe("A. B.");
  });
});
