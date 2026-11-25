// Server rendering of the #7 template parts in plain Bun (news-component-testing, "Server rendering"): every gallery
// case renders complete static HTML with no Vite or Astro in between: no script, no hydration marker, no inline
// handler, no React warning, and stable ids between renders. The library components' pages (Prose, Quote, Note,
// MethodSummary, ImageFigure, ImageGallery, VideoEmbed) render through the renderer in tests/ssr/renderer.test.tsx's
// way here too, from .oui read off disk.
import { describe, expect, test } from "bun:test";
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import type { ReactNode } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { cases as badge } from "../../src/components/Badge/fixtures/cases";
import type { FixtureCase } from "../../src/components/contract";
import { cases as dateLine } from "../../src/components/DateLine/fixtures/cases";
import { cases as disclaimer } from "../../src/components/Disclaimer/fixtures/cases";
import { cases as flyTitle } from "../../src/components/FlyTitle/fixtures/cases";
import { cases as footer } from "../../src/components/Footer/fixtures/cases";
import { cases as glossaryTerm } from "../../src/components/GlossaryTerm/fixtures/cases";
import { cases as headline } from "../../src/components/Headline/fixtures/cases";
import { cases as masthead } from "../../src/components/Masthead/fixtures/cases";
import { cases as pageMeta } from "../../src/components/PageMeta/fixtures/cases";
import { cases as regionFilter } from "../../src/components/RegionFilter/fixtures/cases";
import { cases as skipLink } from "../../src/components/SkipLink/fixtures/cases";
import { cases as sourcesPanel } from "../../src/components/SourcesPanel/fixtures/cases";
import { cases as standfirst } from "../../src/components/Standfirst/fixtures/cases";
import { PageData, PageSources } from "../../src/renderer/bundle/schema";
import { PageDataProvider } from "../../src/renderer/data/context";
import { PageDataResolver } from "../../src/renderer/data/resolve";
import { mergeBundleData } from "../../src/renderer/fixtures/bundles/assemble";
import { readFixtureBundles } from "../../src/renderer/fixtures/bundles/fs";
import { fixtureGlossary } from "../../src/renderer/fixtures/glossary";
import { fixtureLibrary } from "../../src/renderer/fixtures/library";
import { renderPage } from "../../src/renderer/render";

const GALLERIES: Record<string, FixtureCase[]> = {
  Badge: badge,
  DateLine: dateLine,
  Disclaimer: disclaimer,
  FlyTitle: flyTitle,
  Footer: footer,
  GlossaryTerm: glossaryTerm,
  Headline: headline,
  Masthead: masthead,
  PageMeta: pageMeta,
  RegionFilter: regionFilter,
  SkipLink: skipLink,
  SourcesPanel: sourcesPanel,
  Standfirst: standfirst,
};

const bundles = readFixtureBundles();
const siteRoot = join(import.meta.dir, "../..");
const componentData = (file: string) =>
  JSON.parse(readFileSync(join(siteRoot, "src/renderer/fixtures/component-pages", file), "utf8"));
const merged = mergeBundleData(bundles["apple-fy2024"], bundles["edge-cases"], {
  ...bundles.static,
  data: componentData("data.json"),
  sources: componentData("sources.json"),
});
const resolver = () => new PageDataResolver(PageData.parse(merged.data), PageSources.parse(merged.sources));
const render = (ui: ReactNode) => renderToStaticMarkup(<PageDataProvider resolver={resolver()}>{ui}</PageDataProvider>);

function quietly<T>(fn: () => T): { value: T; warnings: string[] } {
  const warnings: string[] = [];
  const original = console.error;
  console.error = (...args: unknown[]) => warnings.push(args.map(String).join(" "));
  try {
    return { value: fn(), warnings };
  } finally {
    console.error = original;
  }
}

describe("#7 template parts under Bun", () => {
  for (const [name, cases] of Object.entries(GALLERIES)) {
    for (const c of cases) {
      test(`${name}: "${c.name}" renders complete static HTML`, () => {
        const { value: html, warnings } = quietly(() => render(c.render()));
        expect(warnings).toEqual([]);
        expect(html.length).toBeGreaterThan(0);
        expect(html).not.toContain("<script");
        expect(html).not.toMatch(/<!--\$|data-reactroot|__react/);
        expect(html).not.toMatch(/\son[a-z]+=/i);
        expect(html).not.toContain("style=");
        expect(html.replace(/<[^>]+>/g, "")).not.toMatch(/(^|[\s(])-\d/);
        expect(render(c.render())).toBe(html);
      });
    }
  }
});

describe("#7 library components under Bun, through the renderer", () => {
  const componentsDir = join(siteRoot, "src/components");
  const ISLANDS = new Set(["ImageGallery", "VideoEmbed"]);
  for (const name of ["Prose", "Quote", "Note", "MethodSummary", "ImageFigure", "ImageGallery", "VideoEmbed"]) {
    for (const file of readdirSync(join(componentsDir, name, "fixtures")).filter((f) => f.endsWith(".oui"))) {
      test(`${name}/${file}: renders; static ones carry no island, islands only their own`, () => {
        const oui = readFileSync(join(componentsDir, name, "fixtures", file), "utf8");
        const { value: result, warnings } = quietly(() =>
          renderPage(
            {
              oui,
              data: merged.data,
              sources: merged.sources,
              head: { flyTitle: "Fixture", headline: `${name} ${file}`, summary: "A fixture." },
            },
            { library: fixtureLibrary(), glossary: fixtureGlossary },
          ),
        );
        expect(warnings).toEqual([]);
        expect(result.html).toContain("</article></main></body></html>");
        expect(result.hasIslands).toBe(ISLANDS.has(name));
        if (!ISLANDS.has(name)) expect(result.html).not.toContain("<script");
        // Nothing is requested from YouTube in the server HTML.
        expect(result.html).not.toMatch(/src="[^"]*youtube/);
      });
    }
  }
});
