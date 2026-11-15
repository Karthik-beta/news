import { experimental_AstroContainer as AstroContainer } from "astro/container";
import { JSDOM } from "jsdom";
import { beforeAll, describe, expect, it } from "vitest";
import IndexPage from "../../src/pages/index.astro";

// Server rendering: the page as HTML, before any JavaScript runs. Parse it with jsdom (no scripts executed)
// and assert on structure, so the test reads the same document a reader without JavaScript gets.
let html: string;
let doc: Document;

beforeAll(async () => {
  const container = await AstroContainer.create();
  html = await container.renderToString(IndexPage, { partial: false });
  doc = new JSDOM(html).window.document;
});

describe("index page (server-rendered)", () => {
  it("is a complete British-English document with a title and viewport", () => {
    expect(html.toLowerCase().startsWith("<!doctype html>")).toBe(true);
    expect(doc.documentElement.lang).toBe("en-GB");
    expect(doc.title.trim()).not.toBe("");
    expect(doc.querySelector('meta[name="viewport"]')?.getAttribute("content")).toContain("width=device-width");
  });

  it("has exactly one h1, inside main, with the fly title as a paragraph before it", () => {
    const h1s = doc.querySelectorAll("h1");
    expect(h1s).toHaveLength(1);
    const h1 = h1s[0] as HTMLHeadingElement;
    expect(h1.closest("main")).not.toBeNull();
    const fly = doc.querySelector("main .fly");
    expect(fly?.tagName).toBe("P");
    expect(fly && fly.compareDocumentPosition(h1) & 4).toBeTruthy(); // DOCUMENT_POSITION_FOLLOWING
  });

  it("ships no JavaScript (static pages are 0 KB JS)", () => {
    const scripts = [...doc.querySelectorAll("script")].filter((s) => s.type !== "application/ld+json");
    expect(scripts).toHaveLength(0);
    expect(doc.querySelector("astro-island")).toBeNull();
  });

  it("has no inline style attributes or hard-coded colours (tokens only)", () => {
    expect(doc.querySelector("[style]")).toBeNull();
    expect(doc.querySelector('[fill^="#"], [stroke^="#"], [color], [bgcolor]')).toBeNull();
  });
});
