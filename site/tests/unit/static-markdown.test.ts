import { describe, expect, it } from "vitest";
import { inlineHtml, parseBlocks, plainText, renderBlocks, stripComments } from "../../src/static/markdown";

describe("static-page Markdown", () => {
  it("parses headings, paragraphs, lists and tables, dropping comments", () => {
    const blocks = parseBlocks(
      [
        "## In short",
        "",
        "One line",
        "joined to the next.",
        "",
        "<!-- TODO(Karthik): a note",
        "over two lines -->",
        "",
        "- a",
        "- b",
        "",
        "1. first",
        "2. second",
        "",
        "### Sub",
        "",
        "| Tool | Returns |",
        "| --- | --- |",
        "| `get_page` | A page |",
      ].join("\n"),
    );
    expect(blocks).toEqual([
      { type: "h2", text: "In short", id: "in-short" },
      { type: "p", text: "One line joined to the next." },
      { type: "ul", items: ["a", "b"] },
      { type: "ol", items: ["first", "second"] },
      { type: "h3", text: "Sub", id: "sub" },
      { type: "table", head: ["Tool", "Returns"], rows: [["`get_page`", "A page"]] },
    ]);
  });

  it("gives repeated headings unique ids", () => {
    const ids = parseBlocks("## Accuracy\n\n## Accuracy\n").map((b) => (b.type === "h2" ? b.id : ""));
    expect(ids).toEqual(["accuracy", "accuracy-2"]);
  });

  it("escapes HTML and renders bold, code and safe links only", () => {
    expect(inlineHtml("<b>x</b> & **bold** `a<b`")).toBe(
      "&lt;b&gt;x&lt;/b&gt; &amp; <strong>bold</strong> <code>a&lt;b</code>",
    );
    expect(inlineHtml("[home](/) [out](https://example.com/a?b=1&c=2) [mail](mailto:a@b.co)")).toBe(
      '<a href="/">home</a> <a href="https://example.com/a?b=1&amp;c=2">out</a> <a href="mailto:a@b.co">mail</a>',
    );
    // javascript:, protocol-relative and relative links lose their link and keep their words
    expect(inlineHtml("[x](javascript:alert) [y](//evil.test) [z](page.html)")).toBe("x y z");
  });

  it("renders a glossary link through the term hook, or as plain words without one", () => {
    expect(inlineHtml("a [core prices](glossary:core)", { term: (w, s) => `<i>${w}:${s}</i>` })).toBe(
      "a <i>core prices:core</i>",
    );
    expect(inlineHtml("a [core prices](glossary:core)")).toBe("a core prices");
  });

  it("reads plain text without markup", () => {
    expect(plainText("**Basis point.** See [methodology](/methodology) and `x`.")).toBe(
      "Basis point. See methodology and x.",
    );
  });

  it("strips comments across lines", () => {
    expect(stripComments("a<!-- x\ny -->b")).toBe("ab");
  });

  it("renders blocks with the Prose classes", () => {
    const html = renderBlocks(parseBlocks("## A\n\n- x\n\n| H |\n| - |\n| r |"));
    expect(html).toContain('<h2 class="prose__heading text-headline-l" id="a">A</h2>');
    expect(html).toContain('<ul class="prose__list"><li>x</li></ul>');
    expect(html).toContain('<th scope="col">H</th>');
    expect(html).toContain('<th scope="row">r</th>');
  });
});
