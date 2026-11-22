import reactRenderer from "@astrojs/react/server.js";
import { experimental_AstroContainer as AstroContainer } from "astro/container";
import { JSDOM } from "jsdom";
import { beforeAll, describe, expect, it } from "vitest";
import { siteConfig } from "../../src/config/site";
import About from "../../src/pages/about.astro";
import Contact from "../../src/pages/contact.astro";
import Corrections from "../../src/pages/corrections.astro";
import Data from "../../src/pages/data.astro";
import Glossary from "../../src/pages/glossary.astro";
import Legal from "../../src/pages/legal.astro";
import Methodology from "../../src/pages/methodology.astro";
import Privacy from "../../src/pages/privacy.astro";
import Report from "../../src/pages/report.astro";
import { glossaryItems } from "../../src/static/content";

// Server rendering of the static pages: the HTML a reader gets before any JavaScript.
const PAGES = [
  ["/methodology/", Methodology],
  ["/about/", About],
  ["/glossary/", Glossary],
  ["/privacy/", Privacy],
  ["/legal/", Legal],
  ["/contact/", Contact],
  ["/report/", Report],
  ["/data/", Data],
  ["/corrections/", Corrections],
] as const;

const docs = new Map<string, Document>();

beforeAll(async () => {
  const container = await AstroContainer.create();
  container.addServerRenderer({ name: "@astrojs/react", renderer: reactRenderer });
  container.addClientRenderer({ name: "@astrojs/react", entrypoint: "@astrojs/react/client.js" });
  for (const [path, page] of PAGES) {
    docs.set(path, new JSDOM(await container.renderToString(page, { partial: false })).window.document);
  }
});

describe.each(PAGES.map(([path]) => path))("static page %s (server-rendered)", (path) => {
  const doc = () => docs.get(path) as Document;

  it("is a British-English document with a title, description and canonical address", () => {
    expect(doc().documentElement.lang).toBe("en-GB");
    expect(doc().title).toMatch(/ · News$/);
    expect(doc().querySelector('meta[name="description"]')?.getAttribute("content")).not.toBe("");
    expect(doc().querySelector('link[rel="canonical"]')?.getAttribute("href")).toBe(`${siteConfig.site.origin}${path}`);
  });

  it("has one h1 in main, a skip link first, the masthead and the footer", () => {
    expect(doc().querySelectorAll("h1")).toHaveLength(1);
    expect(doc().querySelector("main#main h1")).not.toBeNull();
    expect(doc().querySelector("body > a.skip-link")?.getAttribute("href")).toBe("#main");
    expect(doc().querySelector("header.masthead")).not.toBeNull();
    expect(doc().querySelector("footer.site-footer")).not.toBeNull();
  });

  it("ships no JavaScript, inline style or leftover {token}", () => {
    expect(doc().querySelectorAll("script")).toHaveLength(0);
    expect(doc().querySelector("[style]")).toBeNull();
    expect(doc().querySelector("main")?.textContent).not.toMatch(/\{[\w.-]+\}/);
  });

  it("has unique ids and no heading level skipped", () => {
    const ids = [...doc().querySelectorAll("[id]")].map((e) => e.id);
    expect(ids.filter((id, i) => ids.indexOf(id) !== i)).toEqual([]);
    const levels = [...doc().querySelectorAll("main h1, main h2, main h3")].map((h) => Number(h.tagName[1]));
    for (const [i, l] of levels.entries()) if (i > 0) expect(l - (levels[i - 1] as number)).toBeLessThanOrEqual(1);
  });
});

describe("page specifics", () => {
  it("renders the glossary A to Z with a jump row, an id on every term and every letter linked once", () => {
    const doc = docs.get("/glossary/") as Document;
    const items = glossaryItems();
    expect(doc.querySelectorAll(".glossary__term")).toHaveLength(items.length);
    for (const g of items) expect(doc.getElementById(g.slug)?.querySelector("dt")?.textContent).toBe(g.term);
    expect(doc.querySelectorAll(".glossary__jump-list li")).toHaveLength(26);
    for (const a of doc.querySelectorAll<HTMLAnchorElement>("a.glossary__jump-link")) {
      expect(doc.getElementById(a.getAttribute("href")?.slice(1) as string)).not.toBeNull();
    }
  });

  it("renders configured values from config: contact address, legal name, licence, limits", () => {
    const text = (p: string) => (docs.get(p) as Document).querySelector("main")?.textContent ?? "";
    expect(docs.get("/contact/")?.querySelector(`a[href="mailto:${siteConfig.contact.email}"]`)).not.toBeNull();
    expect(text("/legal/")).toContain(siteConfig.operator.legalName);
    expect(docs.get("/legal/")?.querySelector(`a[href="${siteConfig.licence.url}"]`)?.textContent).toBe(
      siteConfig.licence.name,
    );
    expect(text("/data/")).toContain(`answers ${siteConfig.mcp.ratePerMinute} requests a minute`);
    expect(text("/data/")).toContain(siteConfig.mcp.url);
    expect(text("/privacy/")).toContain(`${siteConfig.reports.retentionDays} days`);
  });

  it("opens a glossary definition from a term in the methodology", () => {
    const doc = docs.get("/methodology/") as Document;
    const trigger = doc.querySelector("button.glossary-term");
    expect(trigger).not.toBeNull();
    const panel = doc.getElementById(trigger?.getAttribute("popovertarget") as string);
    expect(panel?.getAttribute("popover")).toBe("auto");
    expect(panel?.querySelector("a")?.getAttribute("href")).toMatch(/^\/glossary\/#[a-z0-9-]+$/);
  });

  it("has no form, points to the contact page, and shows no build notes", () => {
    const doc = docs.get("/report/") as Document;
    expect(doc.querySelector("form")).toBeNull();
    expect(doc.querySelector('main a[href="/contact"]')).not.toBeNull();
    const text = doc.querySelector("main")?.textContent ?? "";
    for (const note of ["Builder notes", "Form text", "What the form stores"]) expect(text).not.toContain(note);
  });
});
