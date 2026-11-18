import { createParser, type ElementNode, type Library, Renderer, useRenderNode } from "@openuidev/react-lang";
import { Fragment, type ReactNode } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { Island } from "./Island";
import { islands } from "./islands";
import { spikeLibrary } from "./library";

// Approach B: OpenUI's own React runtime (Renderer.Root) rendered to static markup with react-dom/server.
// Approach C (the fallback): our own walk over the OpenUI Lang parse tree, no OpenUI React runtime at all.
// Both emit the same HTML; islands are rendered by <Island> and hydrated by hydrate.client.ts.

const parsers = new WeakMap<Library, ReturnType<typeof createParser>>();
function parserFor(library: Library) {
  let parser = parsers.get(library);
  if (!parser) {
    parser = createParser(library.toJSONSchema(), library.root);
    parsers.set(library, parser);
  }
  return parser;
}

export function parseOui(oui: string, library: Library = spikeLibrary) {
  const result = parserFor(library).parse(oui);
  if (!result.root) throw new Error(`OpenUI Lang has no root: ${JSON.stringify(result.meta.errors)}`);
  if (result.meta.errors.length > 0) throw new Error(`OpenUI Lang errors: ${JSON.stringify(result.meta.errors)}`);
  return result.root;
}

/** Renders the parsed root inside OpenUI's runtime, without <Renderer>'s query-loading wrapper divs. */
function RootSlot({ root }: { root: ElementNode }) {
  return <>{useRenderNode()(root)}</>;
}

/** B: OpenUI Renderer.Root → static HTML. */
export function renderWithOpenUI(oui: string, library: Library = spikeLibrary): string {
  const root = parseOui(oui, library);
  return renderToStaticMarkup(
    <Renderer.Root response={oui} library={library}>
      <RootSlot root={root} />
    </Renderer.Root>,
  );
}

/** C (fallback): our own renderer over the OpenUI Lang syntax tree. */
export function renderWithOwnWalker(oui: string): string {
  return renderToStaticMarkup(renderNode(parseOui(oui)));
}

const staticComponents: Record<string, (props: Record<string, unknown>, children: ReactNode) => ReactNode> = {
  Page: (_p, children) => <article className="oui-page">{children}</article>,
  Heading: (p) => <h1 className="oui-heading">{p.text as string}</h1>,
  Paragraph: (p) => <p className="oui-paragraph">{p.text as string}</p>,
  KeyFigure: (p) => (
    <div className="oui-figure">
      <p className="oui-figure-label">{p.label as string}</p>
      <p className="oui-figure-value">{p.value as string}</p>
      <p className="oui-figure-change">{p.change as string}</p>
    </div>
  ),
};

function renderNode(value: unknown): ReactNode {
  if (Array.isArray(value)) return value.map(renderNode);
  if (!value || typeof value !== "object" || (value as ElementNode).type !== "element") return value as ReactNode;
  const node = value as ElementNode;
  const key = node.statementId;
  if (node.typeName in islands) {
    return <Island key={key} name={node.typeName as keyof typeof islands} props={node.props} />;
  }
  const render = staticComponents[node.typeName];
  if (!render) throw new Error(`Unknown component ${node.typeName}`);
  const { children, ...props } = node.props as Record<string, unknown>;
  return <Fragment key={key}>{render(props, renderNode(children))}</Fragment>;
}

/** A complete HTML document around the rendered body, the way the page bundle's index.html would be written. */
export function renderDocument(body: string, opts: { title: string; css: string; script?: string }): string {
  const hasIslands = body.includes("data-island=");
  return [
    '<!doctype html><html lang="en-GB"><head><meta charset="utf-8">',
    '<meta name="viewport" content="width=device-width, initial-scale=1">',
    `<title>${opts.title}</title><link rel="stylesheet" href="${opts.css}">`,
    hasIslands && opts.script ? `<script type="module" src="${opts.script}"></script>` : "",
    `</head><body><main>${body}</main></body></html>`,
  ].join("");
}
