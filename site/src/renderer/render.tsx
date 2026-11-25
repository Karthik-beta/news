// The page renderer (ADR 0001, decision B). Turns a page bundle (page.oui, data.json, sources.json, the head
// copy) into index.html, index.md and a CSV and JSON file per chart, in Bun or Node, with no Astro and no Vite.
//
//   1. validate data.json and sources.json against the bundle contract, and check their cross-references;
//   2. parse page.oui with OpenUI's parser against the component library;
//   3. render the article with react-dom/server inside OpenUI's Renderer.Root and our own root slot (never the
//      default <Renderer>, which adds wrapper divs with inline styles); interactive parts are islands;
//   4. write Markdown and chart downloads through each component's hooks.
//
// The fallback (C) walks the parse tree itself and must produce the same HTML; `engine: "walker"` selects it and a
// test keeps the two identical. Contract and data references: README.md in this folder.
import { createParser, type ElementNode, type Library, Renderer, useRenderNode } from "@openuidev/react-lang";
import { Fragment, type ReactNode } from "react";
import type { AnyComponentSpec, ChartTable } from "../components/contract";
import { Disclaimer, type DisclaimerKind } from "../components/Disclaimer/Disclaimer";
import { type Glossary, GlossaryProvider } from "../components/GlossaryTerm/GlossaryTerm";
import { type NewsLibrary, newsLibrary } from "../components/library";
import { RefText } from "../components/SourceRef/RefText";
import { formatDate } from "../format";
import { PageData, type PageDataT, PageSources, type PageSourcesT } from "./bundle/schema";
import { PageDataProvider } from "./data/context";
import { PageDataResolver, type Problem } from "./data/resolve";
import { chartFile, mayRedistribute, toChartJson, toCsv } from "./exports";
import { markdownOf, pageMarkdown, sentence } from "./markdown";
import { staticMarkup } from "./markup";
import { RENDERER_NAME, RENDERER_VERSION, rendererTag } from "./version";

/** The copy that sits around page.oui: written by the page writer with `{ref}` tokens, filled here by code. */
export interface PageHead {
  flyTitle: string;
  headline: string;
  summary: string;
}

export interface PageBundleInput {
  oui: string;
  /** data.json, parsed (validated here). */
  data: unknown;
  /** sources.json, parsed (validated here). */
  sources: unknown;
  head: PageHead;
}

export interface RenderOptions {
  /** The component library; production by default. Tests and fixture pages pass their own. */
  library?: NewsLibrary;
  /** "openui" (decision B, the default) or "walker" (fallback C). */
  engine?: "openui" | "walker";
  /** Throw on any data problem (unknown reference, missing value) instead of rendering the missing state. */
  strict?: boolean;
  /** Stylesheets and the island script for the full document (hashed shell assets). */
  assets?: { stylesheets: string[]; script?: string; preloadFonts?: string[] };
  /** Language of the document. */
  lang?: string;
  /**
   * The site glossary, for glossary terms in page copy. Without it, terms render as plain text (a definition is
   * never made up). Page templates (#14) pass the real one; fixture pages pass a fixture glossary.
   */
  glossary?: Glossary;
  /**
   * The page's index_status, written as its one robots meta tag. Default noindex, so a preview or fixture page is never
   * indexed. The write API refuses a bundle without exactly one of the two tags and rewrites it when the status
   * changes (ADR 0004, "Index status").
   */
  indexStatus?: "index" | "noindex";
  /** The disclaimer the page's categories need (the brief's rule), after the sources. Code adds it, never the writer. */
  disclaimer?: DisclaimerKind | null;
}

export interface ChartExport {
  statementId: string;
  table: ChartTable;
  csv: string;
  json: string;
}

export interface RenderResult {
  /** The full document (index.html). */
  html: string;
  /** The <article> alone, for hosting inside another layout (Astro fixture pages). */
  article: string;
  markdown: string;
  charts: ChartExport[];
  /** Bundle files by relative path: index.html, index.md, charts/<id>.csv, charts/<id>.json. */
  files: Record<string, string>;
  /** True when the page has at least one island, so it needs the client script. */
  hasIslands: boolean;
  rendererVersion: string;
  refsUsed: string[];
  sourcesUsed: string[];
  /** Data problems found (unknown references, missing values, broken links). Empty for a publishable page. */
  problems: Problem[];
}

export class RenderError extends Error {
  constructor(
    readonly code: "invalid-data" | "invalid-sources" | "parse" | "data-problems",
    message: string,
    readonly details: unknown = null,
  ) {
    super(message);
    this.name = "RenderError";
  }
}

const parsers = new WeakMap<Library, ReturnType<typeof createParser>>();
function parserFor(library: Library) {
  let parser = parsers.get(library);
  if (!parser) {
    parser = createParser(library.toJSONSchema(), library.root);
    parsers.set(library, parser);
  }
  return parser;
}

/** Parses page.oui; any OpenUI Lang error is a RenderError (check C1). */
export function parsePage(oui: string, library: Library): ElementNode {
  const result = parserFor(library).parse(oui);
  const errors = result.meta.errors;
  if (!result.root || errors.length > 0 || result.meta.unresolved.length > 0) {
    const unresolved = result.meta.unresolved.map((name) => `"${name}" is not defined (write references as strings)`);
    const messages = [...errors.map((e) => `${e.statementId ?? "?"}: ${e.message}`), ...unresolved];
    throw new RenderError("parse", `page.oui does not parse:\n  ${messages.join("\n  ") || "no root"}`, result.meta);
  }
  return result.root;
}

/** True when a prop path runs into a child element: the element's own props are checked when it is visited. */
function insideElement(props: unknown, path: readonly PropertyKey[]): boolean {
  let value: unknown = props;
  for (const key of path.slice(0, -1)) {
    if (!value || typeof value !== "object") return false;
    value = (value as Record<PropertyKey, unknown>)[key];
    if (value && typeof value === "object" && (value as ElementNode).type === "element") return true;
  }
  return false;
}

/**
 * Checks every element's props against its component's zod schema. OpenUI's parser checks types and required
 * arguments but not refinements, so a typed number where a reference belongs ("391.0" for a dp_… prop) is caught
 * here (check C1).
 */
export function checkProps(root: ElementNode, library: Library): void {
  const problems: string[] = [];
  const visit = (value: unknown): void => {
    if (Array.isArray(value)) {
      value.forEach(visit);
      return;
    }
    if (!value || typeof value !== "object" || (value as ElementNode).type !== "element") return;
    const node = value as ElementNode;
    const schema = library.components[node.typeName]?.props as
      | {
          safeParse: (v: unknown) => {
            success: boolean;
            error?: { issues: { path: PropertyKey[]; message: string }[] };
          };
        }
      | undefined;
    const result = schema?.safeParse(node.props);
    if (result && !result.success) {
      for (const issue of result.error?.issues ?? []) {
        // Child elements are checked on their own; the number of children (KeyFigures' 2-4) is checked here.
        if (issue.path[0] === "children" && issue.path.length > 1) continue;
        // So is a single child element in another prop (PolicyDecision's StepLineChart `path`).
        if (insideElement(node.props, issue.path)) continue;
        problems.push(`${node.statementId ?? node.typeName}.${issue.path.join(".")}: ${issue.message}`);
      }
    }
    for (const v of Object.values(node.props)) visit(v);
  };
  visit(root);
  if (problems.length > 0)
    throw new RenderError("parse", `page.oui has invalid props:\n  ${problems.join("\n  ")}`, problems);
}

/** Renders the parsed root inside OpenUI's runtime, without <Renderer>'s wrapper divs. */
function RootSlot({ root }: { root: ElementNode }) {
  return <>{useRenderNode()(root)}</>;
}

/** Fallback C: our own walk over the parse tree, mapping each element to its component. */
function walk(value: unknown, library: Library): ReactNode {
  if (Array.isArray(value)) return value.map((v) => walk(v, library));
  if (!value || typeof value !== "object" || (value as ElementNode).type !== "element") return value as ReactNode;
  const node = value as ElementNode;
  const Component = library.components[node.typeName]?.component as
    | ((p: { props: unknown; renderNode: (v: unknown) => ReactNode; statementId?: string }) => ReactNode)
    | undefined;
  if (!Component) throw new RenderError("parse", `Unknown component ${node.typeName}`);
  return (
    <Fragment key={node.statementId}>
      <Component props={node.props} renderNode={(v) => walk(v, library)} statementId={node.statementId} />
    </Fragment>
  );
}

function sourceIdsInOrder(resolver: PageDataResolver): string[] {
  return [...resolver.usedSources];
}

export function SourcesList({ ids, resolver }: { ids: string[]; resolver: PageDataResolver }) {
  return (
    <section className="article__sources" aria-labelledby="article-sources">
      <h2 className="text-headline-l" id="article-sources">
        Sources
      </h2>
      <ol className="article__source-list">
        {ids.map((id) => {
          const s = resolver.source(id);
          if (!s) return null;
          return (
            <li key={id} id={`source-${id}`} className="text-label">
              <a className="article__source-title" href={s.url}>
                {s.title}
              </a>
              <span className="article__source-meta">
                {sentence(s.publisher)} {sentence(s.licence.name)} Retrieved {formatDate(s.retrieved_at)}.
              </span>
            </li>
          );
        })}
      </ol>
    </section>
  );
}

function Article({
  head,
  body,
  sourceIds,
  resolver,
  disclaimer,
}: {
  head: PageHead;
  body: ReactNode;
  sourceIds: () => string[];
  resolver: PageDataResolver;
  disclaimer?: DisclaimerKind | null;
}) {
  return (
    <article className="article" data-renderer={rendererTag()}>
      <header className="article__head">
        <p className="text-fly">
          <RefText text={head.flyTitle} cite="plain" />
        </p>
        <h1 className="text-headline-xl">
          <RefText text={head.headline} cite="plain" />
        </h1>
        <p className="text-standfirst">
          <RefText text={head.summary} />
        </p>
      </header>
      {body}
      <LateSources ids={sourceIds} resolver={resolver} />
      {disclaimer ? <Disclaimer kind={disclaimer} /> : null}
    </article>
  );
}

/** Rendered last, so it lists every source the head and body cited while they rendered. */
function LateSources({ ids, resolver }: { ids: () => string[]; resolver: PageDataResolver }) {
  return <SourcesList ids={ids()} resolver={resolver} />;
}

function validate(input: PageBundleInput): { data: PageDataT; sources: PageSourcesT } {
  const data = PageData.safeParse(input.data);
  if (!data.success) {
    throw new RenderError(
      "invalid-data",
      `data.json breaks the bundle contract:\n${data.error.message}`,
      data.error.issues,
    );
  }
  const sources = PageSources.safeParse(input.sources);
  if (!sources.success) {
    throw new RenderError(
      "invalid-sources",
      `sources.json breaks the bundle contract:\n${sources.error.message}`,
      sources.error.issues,
    );
  }
  return { data: data.data, sources: sources.data };
}

function collectCharts(
  value: unknown,
  specs: Map<string, AnyComponentSpec>,
  resolver: PageDataResolver,
  out: { statementId: string; table: ChartTable }[],
): void {
  if (Array.isArray(value)) {
    for (const v of value) collectCharts(v, specs, resolver, out);
    return;
  }
  if (!value || typeof value !== "object" || (value as ElementNode).type !== "element") return;
  const node = value as ElementNode;
  const spec = specs.get(node.typeName);
  if (spec?.csv && node.statementId) {
    const table = spec.csv({
      props: node.props as never,
      statementId: node.statementId,
      data: resolver,
      children: () => "",
    });
    if (table) out.push({ statementId: node.statementId, table });
  }
  for (const v of Object.values(node.props)) collectCharts(v, specs, resolver, out);
}

/** Renders a page bundle. Throws RenderError for an invalid bundle or page.oui; data problems are returned. */
export function renderPage(input: PageBundleInput, options: RenderOptions = {}): RenderResult {
  const { library, specs } = options.library ?? newsLibrary();
  const { data, sources } = validate(input);
  const resolver = new PageDataResolver(data, sources);
  resolver.checkIntegrity();
  const root = parsePage(input.oui, library);
  checkProps(root, library);

  const body =
    options.engine === "walker" ? (
      walk(root, library)
    ) : (
      <Renderer.Root response={input.oui} library={library}>
        <RootSlot root={root} />
      </Renderer.Root>
    );
  const article = staticMarkup(
    <PageDataProvider resolver={resolver}>
      <GlossaryProvider glossary={options.glossary ?? {}}>
        <Article
          head={input.head}
          body={body}
          sourceIds={() => sourceIdsInOrder(resolver)}
          resolver={resolver}
          disclaimer={options.disclaimer}
        />
      </GlossaryProvider>
    </PageDataProvider>,
  );

  const charts: { statementId: string; table: ChartTable }[] = [];
  collectCharts(root, specs, resolver, charts);
  const exports: ChartExport[] = charts
    .filter(({ table }) =>
      mayRedistribute(
        table.sources.flatMap((id) => {
          const source = resolver.source(id);
          return source ? [{ id, source }] : [];
        }),
      ),
    )
    .map(({ statementId, table }) => ({ statementId, table, csv: toCsv(table), json: toChartJson(table, sources) }));

  const bodyMarkdown = markdownOf(root, specs, resolver);
  const markdown = pageMarkdown(
    {
      flyTitle: resolver.plainText(input.head.flyTitle),
      headline: resolver.plainText(input.head.headline),
      summary: resolver.plainText(input.head.summary),
    },
    bodyMarkdown,
    resolver,
    sourceIdsInOrder(resolver),
    rendererTag(),
  );

  if (options.strict && resolver.problems.length > 0) {
    throw new RenderError(
      "data-problems",
      `The page has data problems:\n  ${resolver.problems.map((p) => p.message).join("\n  ")}`,
      resolver.problems,
    );
  }

  const hasIslands = article.includes("data-island=");
  const html = renderDocument({
    title: resolver.plainText(input.head.headline),
    description: resolver.plainText(input.head.summary),
    article,
    lang: options.lang ?? "en-GB",
    assets: options.assets,
    hasIslands,
    indexStatus: options.indexStatus ?? "noindex",
  });

  const files: Record<string, string> = { "index.html": html, "index.md": markdown };
  for (const c of exports) {
    files[chartFile(c.statementId, "csv")] = c.csv;
    files[chartFile(c.statementId, "json")] = c.json;
  }

  return {
    html,
    article,
    markdown,
    charts: exports,
    files,
    hasIslands,
    rendererVersion: RENDERER_VERSION,
    refsUsed: [...resolver.usedRefs].sort(),
    sourcesUsed: [...resolver.usedSources].sort(),
    problems: resolver.problems,
  };
}

const escapeHtml = (s: string) =>
  s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c] as string);

/**
 * The complete index.html around the article. The island script is added only when the page has an island, so
 * a static page ships no JavaScript at all.
 */
export function renderDocument(opts: {
  title: string;
  description: string;
  article: string;
  lang: string;
  assets?: RenderOptions["assets"];
  hasIslands: boolean;
  indexStatus: "index" | "noindex";
}): string {
  const assets = opts.assets ?? { stylesheets: [] };
  return [
    "<!doctype html>",
    `<html lang="${escapeHtml(opts.lang)}">`,
    "<head>",
    '<meta charset="utf-8">',
    '<meta name="viewport" content="width=device-width, initial-scale=1">',
    // Exactly one tag, in one of these two forms: the write API finds and rewrites it byte for byte (workers/api).
    `<meta name="robots" content="${opts.indexStatus === "index" ? "index, follow" : "noindex"}">`,
    `<meta name="generator" content="${RENDERER_NAME} ${RENDERER_VERSION}">`,
    `<title>${escapeHtml(opts.title)}</title>`,
    `<meta name="description" content="${escapeHtml(opts.description)}">`,
    ...(assets.preloadFonts ?? []).map(
      (href) => `<link rel="preload" href="${escapeHtml(href)}" as="font" type="font/woff2" crossorigin>`,
    ),
    ...assets.stylesheets.map((href) => `<link rel="stylesheet" href="${escapeHtml(href)}">`),
    opts.hasIslands && assets.script ? `<script type="module" src="${escapeHtml(assets.script)}"></script>` : "",
    "</head>",
    `<body><main id="main" class="article-page">${opts.article}</main></body>`,
    "</html>",
  ].join("");
}
