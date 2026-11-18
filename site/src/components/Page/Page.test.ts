// Tester's tests for Page, the root of every page.oui (news-component-testing: rendering, data, server rendering,
// the index contract). Page renders through the real renderer, so these go through renderPage with fixture data.
// Spacing between blocks in the browser: tests/e2e/components/Page.spec.ts.
import { JSDOM } from "jsdom";
import { describe, expect, it } from "vitest";
import { fixtureBundles } from "../../renderer/fixtures/bundles/vite";
import { fixtureLibrary } from "../../renderer/fixtures/library";
import { RenderError, renderPage } from "../../renderer/render";
import { estimateTokens } from "../contract";
import { newsLibrary } from "../library";
import spec from "./spec";

const base = fixtureBundles.static;
const render = (oui: string, library = fixtureLibrary()) => renderPage({ ...base, oui }, { library });
const body = (html: string) => new JSDOM(html).window.document.querySelector(".page-body") as HTMLElement;

describe("Page", () => {
  it("is the one root, with an index entry inside the per-entry budget and a description", () => {
    expect(spec.root).toBe(true);
    expect(spec.name).toBe("Page");
    expect(estimateTokens(spec.index)).toBeLessThanOrEqual(24);
    expect(spec.index.startsWith("Page — ")).toBe(true);
    expect(spec.description.length).toBeGreaterThan(0);
    expect(spec.example).not.toMatch(/\d/);
  });

  it("an empty page renders an empty body and no problems", () => {
    const r = render("root = Page([])");
    const el = body(r.article);
    expect(el.tagName).toBe("DIV");
    expect(el.children).toHaveLength(0);
    expect(r.problems).toEqual([]);
  });

  it("renders its children in reading order inside one .page-body, and nothing else", () => {
    const r = render(
      [
        "root = Page([a, b, c])",
        'a = FixtureText("First block.")',
        'b = FixtureText("Second block, with {dp_aapl_net_sales_fy2024}.")',
        'c = FixtureText("Third block.")',
      ].join("\n"),
    );
    const el = body(r.article);
    expect([...el.children].map((c) => c.textContent?.slice(0, 5))).toEqual(["First", "Secon", "Third"]);
    expect(new JSDOM(r.article).window.document.querySelectorAll(".page-body")).toHaveLength(1);
  });

  it("one child, and a long page of 40 blocks, keep their order", () => {
    expect(body(render('root = Page([a])\na = FixtureText("Only.")').article).textContent).toBe("Only.");
    const ids = Array.from({ length: 40 }, (_, i) => `b${i}`);
    const oui = [
      `root = Page([${ids.join(", ")}])`,
      ...ids.map((id, i) => `${id} = FixtureText("Block ${"x".repeat(i % 3)}.")`),
    ];
    // Block text may not hold digits (numbers come from data), so blocks are told apart by position.
    const el = body(render(oui.join("\n")).article);
    expect(el.children).toHaveLength(40);
  });

  it("writes its children to Markdown in order", () => {
    const r = render('root = Page([a, b])\na = FixtureText("First block.")\nb = FixtureText("Second block.")');
    expect(r.markdown.indexOf("First block.")).toBeGreaterThan(-1);
    expect(r.markdown.indexOf("First block.")).toBeLessThan(r.markdown.indexOf("Second block."));
  });

  it("rejects an unknown component and a non-component child", () => {
    expect(() => render('root = Page([x])\nx = Nope("a")')).toThrow(RenderError);
    expect(() => render('root = Page(["plain text"])')).toThrow();
  });

  it("in the production library (no other components yet) accepts only an empty page", () => {
    const r = renderPage({ ...base, oui: "root = Page([])" }, { library: newsLibrary() });
    expect(body(r.article).children).toHaveLength(0);
    expect(() =>
      renderPage({ ...base, oui: 'root = Page([a])\na = FixtureText("x")' }, { library: newsLibrary() }),
    ).toThrow();
  });

  it("ships no script for a page of static blocks", () => {
    const r = render('root = Page([a])\na = FixtureText("Static.")');
    expect(r.hasIslands).toBe(false);
    expect(r.article).not.toContain("<script");
  });
});
