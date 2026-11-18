// index.md: the page as Markdown for LLMs and readers who want text. Each component writes its own Markdown
// through its spec's markdown() hook; the renderer adds the head and the sources.
import type { ElementNode } from "@openuidev/react-lang";
import type { AnyComponentSpec } from "../components/contract";
import { formatDate } from "../format";
import type { PageDataResolver } from "./data/resolve";

/** Ends a phrase with a full stop unless it already has one ("Apple Inc."). */
export const sentence = (s: string) => (/[.!?]$/.test(s.trim()) ? s.trim() : `${s.trim()}.`);

/** Escapes text so Markdown shows it literally. */
export function escapeMarkdown(text: string): string {
  return text.replace(/([\\`*_[\]<>|])/g, "\\$1").replace(/^([#+-]|\d+\.)(?=\s)/gm, "\\$1");
}

const isElement = (v: unknown): v is ElementNode =>
  typeof v === "object" && v !== null && (v as ElementNode).type === "element";

/** Markdown for any value in the tree: elements through their hooks, arrays joined by blank lines. */
export function markdownOf(value: unknown, specs: Map<string, AnyComponentSpec>, data: PageDataResolver): string {
  const children = (v: unknown) => markdownOf(v, specs, data);
  if (Array.isArray(value)) {
    return value
      .map(children)
      .filter((s) => s.trim() !== "")
      .join("\n\n");
  }
  if (!isElement(value)) return "";
  const spec = specs.get(value.typeName);
  if (spec?.markdown) {
    return spec.markdown({ props: value.props as never, statementId: value.statementId, data, children });
  }
  // No hook: write out whatever child elements the props hold, in order.
  return Object.values(value.props).map(children).filter(Boolean).join("\n\n");
}

export interface MarkdownHead {
  flyTitle: string;
  headline: string;
  summary: string;
}

/** The whole index.md: front matter, head, body and sources. */
export function pageMarkdown(
  head: MarkdownHead,
  body: string,
  data: PageDataResolver,
  sourceIds: string[],
  generator: string,
): string {
  const yaml = (s: string) => JSON.stringify(s);
  const sources = sourceIds.flatMap((id) => {
    const s = data.source(id);
    if (!s) return [];
    return [
      `- [${escapeMarkdown(s.title)}](${s.url}). ${escapeMarkdown(sentence(s.publisher))} ${escapeMarkdown(sentence(s.licence.name))} Retrieved ${formatDate(s.retrieved_at)}.`,
    ];
  });
  return [
    "---",
    `title: ${yaml(head.headline)}`,
    `fly_title: ${yaml(head.flyTitle)}`,
    `summary: ${yaml(head.summary)}`,
    `generator: ${yaml(generator)}`,
    "---",
    "",
    escapeMarkdown(head.flyTitle),
    "",
    `# ${escapeMarkdown(head.headline)}`,
    "",
    escapeMarkdown(head.summary),
    "",
    ...(body.trim() ? [body.trim(), ""] : []),
    "## Sources",
    "",
    ...(sources.length ? sources : ["No sources."]),
    "",
  ].join("\n");
}
