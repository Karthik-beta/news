// Builder's checks for the #7 family (chrome, article head, text, media, sources): the page-copy markup, Markdown
// snapshots of every component fixture, the index details, and the crimson rule. The formal tests and sign-off are
// the component tester's (news-component-testing skill).
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { buildComponentIndex, componentSpecs } from "../../src/components/library";
import { latestRetrieval, pageMetaMarkdown } from "../../src/components/PageMeta/PageMeta";
import { copyParts, parseBlocks } from "../../src/components/Prose/copy";
import { specs } from "../../src/components/registry.generated";
import { sourcesPanelMarkdown } from "../../src/components/SourcesPanel/SourcesPanel";
import { PageSources } from "../../src/renderer/bundle/schema";
import { fixtureBundles } from "../../src/renderer/fixtures/bundles/vite";
import { componentFixtures, renderComponentFixture } from "../../src/renderer/fixtures/component-pages";
import { fixtureLibrary } from "../../src/renderer/fixtures/library";
import { RenderError, renderPage } from "../../src/renderer/render";
import { siteRoot } from "../support/tokens";

/** The folders this family added. */
const FAMILY = {
  library: ["Prose", "Quote", "Note", "MethodSummary", "ImageFigure", "ImageGallery", "VideoEmbed"],
  parts: [
    "GlossaryTerm",
    "SkipLink",
    "Masthead",
    "DateLine",
    "RegionFilter",
    "Footer",
    "FlyTitle",
    "Headline",
    "Standfirst",
    "PageMeta",
    "Disclaimer",
    "Badge",
    "SourcesPanel",
  ],
} as const;
const ALL = [...FAMILY.library, ...FAMILY.parts];

describe("page copy markup", () => {
  it("splits blocks into paragraphs, headings and lists", () => {
    expect(
      parseBlocks(["## A heading", "- one\n- two", "A paragraph\nwith a break.\n\nA second one.", "  ", ""]),
    ).toEqual([
      { type: "heading", text: "A heading" },
      { type: "list", items: ["one", "two"] },
      { type: "paragraph", text: "A paragraph with a break." },
      { type: "paragraph", text: "A second one." },
    ]);
  });

  it("treats a block with any line not starting '- ' as a paragraph", () => {
    expect(parseBlocks(["- one\nnot an item"])).toEqual([{ type: "paragraph", text: "- one not an item" }]);
  });

  it("finds glossary terms and leaves other brackets alone", () => {
    expect(copyParts("A [core rate](glossary:core-inflation) and [other](https://x.org) [text].")).toEqual([
      { type: "text", text: "A " },
      { type: "term", words: "core rate", slug: "core-inflation" },
      { type: "text", text: " and [other](https://x.org) [text]." },
    ]);
  });
});

describe("component fixtures", () => {
  it("every library component in this family has a default and an edge-case fixture", () => {
    for (const name of FAMILY.library) {
      const fixtures = componentFixtures.filter((f) => f.component === name).map((f) => f.fixture);
      expect(fixtures, name).toEqual(expect.arrayContaining(["default", "edge-cases"]));
    }
  });

  it.each(componentFixtures.map((f) => [`${f.component}/${f.fixture}`, f] as const))(
    "%s renders through the renderer and its Markdown matches the snapshot",
    async (_name, f) => {
      const result = renderComponentFixture(f.component, f.fixture);
      expect(result.article).toContain('class="page-body"');
      await expect(result.markdown).toMatchFileSnapshot(
        join(siteRoot, "src/components", f.component, "fixtures/__snapshots__", `${f.fixture}.md`),
      );
    },
  );

  it("default fixtures are clean: every reference resolves", () => {
    for (const f of componentFixtures.filter((x) => x.fixture === "default")) {
      expect(renderComponentFixture(f.component, f.fixture).problems, `${f.component}/default`).toEqual([]);
    }
  });

  it("static components ship no island; the gallery and the embeddable video do", () => {
    const islands = (c: string, x: string) => renderComponentFixture(c, x).hasIslands;
    for (const name of ["Prose", "Quote", "Note", "MethodSummary", "ImageFigure"]) {
      expect(islands(name, "default"), name).toBe(false);
      expect(islands(name, "edge-cases"), name).toBe(false);
    }
    expect(islands("ImageGallery", "default")).toBe(true);
    expect(islands("VideoEmbed", "default")).toBe(true);
  });

  it("a video before the click requests nothing from YouTube: no YouTube URL in a src", () => {
    const { article } = renderComponentFixture("VideoEmbed", "default");
    expect(article).not.toMatch(/src="[^"]*youtube/);
    expect(article).toContain("Loads from YouTube. YouTube will receive your IP address.");
  });

  it("missing data reads 'No data' and is reported, never a zero", () => {
    const quote = renderComponentFixture("Quote", "edge-cases");
    expect(quote.article).toMatch(/quote--missing[^>]*>No data</);
    expect(quote.problems.map((p) => p.ref)).toContain("q_not_in_data");
    const prose = renderComponentFixture("Prose", "edge-cases");
    expect(prose.problems.map((p) => p.code)).toEqual(expect.arrayContaining(["no-value", "unknown-ref"]));
  });

  it("a glossary term the glossary defines opens a native popover; an unknown one is plain text", () => {
    const { article } = renderComponentFixture("Prose", "edge-cases");
    expect(article).toMatch(
      /<button type="button" class="popover-trigger glossary-term" popovertarget="[^"]+">core inflation<\/button>/,
    );
    expect(article).toContain("One it does not: neutral rate.");
  });

  it("the renderer refuses typed numbers and bad props for these components (check C1)", () => {
    const render = (oui: string) =>
      renderPage({ ...fixtureBundles["apple-fy2024"], oui }, { library: fixtureLibrary() });
    expect(() => render('root = Page([q])\nq = Quote("391.0")')).toThrow(RenderError);
    expect(() => render("root = Page([p])\np = Prose([])")).toThrow(/blocks/);
    expect(() =>
      render('root = Page([i])\ni = ImageFigure("javascript:alert(1)", "Alt.", 10, 10, "Credit", "Public domain")'),
    ).toThrow(/src/);
    expect(() => render('root = Page([v])\nv = VideoEmbed("T", "P", "https://example.org/", "not-an-id")')).toThrow(
      /youtubeId/,
    );
    const one = '{src: "/a.svg", alt: "A.", width: 1, height: 1, credit: "C", licence: "L"}';
    expect(() => render(`root = Page([g])\ng = ImageGallery([${one}])`)).toThrow(/images/);
  });
});

describe("template parts' Markdown", () => {
  const sources = PageSources.parse(fixtureBundles["apple-fy2024"].sources).sources;
  const cited = Object.entries(sources).map(([id, source]) => ({ id, source }));

  it("PageMeta: byline, data date, topics and update", () => {
    expect(
      pageMetaMarkdown({
        dataAsOf: latestRetrieval(cited.map((c) => c.source)),
        fiscalNote: "Apple's fiscal year ends in September",
        topics: [
          { label: "Earnings", href: "/finance/earnings/" },
          { label: "Apple", href: "/companies/apple/" },
        ],
        updatedOn: "2024-12-03",
      }),
    ).toBe(
      [
        "Built with Claude (AI) · Automated checks · Run by Karthik",
        "Data as of 1 November 2024. Apple's fiscal year ends in September.",
        "Earnings, Apple",
        "Updated on 3 December 2024",
      ].join("\\\n"),
    );
  });

  it("SourcesPanel: items, change note, accuracy note and disclaimer", async () => {
    await expect(
      sourcesPanelMarkdown({
        sources: cited,
        changeNote: { date: "2024-12-03", text: "Corrected a period label" },
        disclaimer: "investment",
      }),
    ).toMatchFileSnapshot(join(siteRoot, "src/components/SourcesPanel/fixtures/__snapshots__/panel.md"));
  });

  it("latestRetrieval takes the latest date, date-times included", () => {
    expect(latestRetrieval([{ retrieved_at: "2024-11-01" }, { retrieved_at: "2024-11-13T09:00:00Z" }])).toBe(
      "2024-11-13",
    );
    expect(latestRetrieval([])).toBeNull();
  });
});

describe("component index details (news component <name>)", () => {
  it("every library component carries description, dataShape, example and a props JSON Schema", () => {
    const index = buildComponentIndex(componentSpecs(specs));
    for (const name of FAMILY.library) {
      const entry = index.components.find((c) => c.name === name);
      expect(entry, name).toBeDefined();
      expect(entry?.description.length, name).toBeGreaterThan(20);
      expect(entry?.dataShape.length, name).toBeGreaterThan(10);
      expect(entry?.example, name).toMatch(new RegExp(`^\\w+ = ${name}\\(`));
      expect(entry?.props, name).toMatchObject({ type: "object", properties: expect.any(Object) });
    }
    expect(index.components.find((c) => c.name === "Page")?.props).toBeUndefined();
  });

  it("each example parses and passes the props check", () => {
    for (const s of componentSpecs(specs).filter((x) => (FAMILY.library as readonly string[]).includes(x.name))) {
      const statement = s.example.split(" = ")[0];
      const oui = `root = Page([${statement}])\n${s.example}`;
      expect(
        () => renderPage({ ...fixtureBundles["apple-fy2024"], oui }, { library: fixtureLibrary() }),
        s.name,
      ).not.toThrow();
    }
  });

  it("the template parts stay out of the page writer's library and index", () => {
    const names = componentSpecs(specs).map((s) => s.name);
    for (const part of FAMILY.parts) expect(names).not.toContain(part);
  });
});

describe("crimson is brand and fly titles only (design/color.md)", () => {
  const dir = join(siteRoot, "src/components");
  const BRAND = /var\(--(?:color-brand|chart-highlight)\)/;

  it.each(ALL)("%s uses crimson only for the wordmark and the current-navigation underline", (name) => {
    const css = readFileSync(join(dir, name, `${name}.css`), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
    for (const m of css.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
      const [selector, body] = [m[1]?.trim() ?? "", m[2] ?? ""];
      if (!BRAND.test(body)) continue;
      expect(selector, `${name}: ${selector}`).toMatch(/wordmark|aria-current/);
    }
  });

  it("only FlyTitle sets the fly style and only Masthead the wordmark style", () => {
    for (const name of ALL) {
      const files = readdirSync(join(dir, name)).filter((f) => f.endsWith(".tsx") && !f.startsWith("fixtures"));
      const tsx = files.map((f) => readFileSync(join(dir, name, f), "utf8")).join("\n");
      if (name !== "FlyTitle") expect(tsx, name).not.toContain("text-fly");
      if (name !== "Masthead") expect(tsx, name).not.toContain("text-wordmark");
    }
  });
});
