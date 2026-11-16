// Page copy: the small markup the page writer uses inside Prose, Note and MethodSummary text.
//
//   Blocks (Prose): one string per block. "## " starts a section heading; lines that all start with "- " are a
//   list; anything else is a paragraph (single line breaks inside it read as spaces). A blank line inside a string
//   splits it into more blocks.
//   Inline (every copy string): {ref} tokens for data (RefText: values, periods, quotes, context lines) and
//   [words](glossary:slug) for a glossary term. Nothing else is markup: other brackets and asterisks print as typed.
import type { ReactNode } from "react";
import type { PageDataResolver } from "../../renderer/data/resolve";
import { escapeMarkdown } from "../../renderer/markdown";
import { GlossaryTerm } from "../GlossaryTerm/GlossaryTerm";
import { RefText } from "../SourceRef/RefText";

/** [words](glossary:slug) */
export const GLOSSARY_LINK = /\[([^[\]\n]+)\]\(glossary:([a-z0-9]+(?:-[a-z0-9]+)*)\)/g;

export type CopyPart = { type: "text"; text: string } | { type: "term"; words: string; slug: string };

/** Splits copy into plain runs (which may hold {ref} tokens) and glossary terms. */
export function copyParts(text: string): CopyPart[] {
  const out: CopyPart[] = [];
  let last = 0;
  for (const m of text.matchAll(GLOSSARY_LINK)) {
    if (m.index > last) out.push({ type: "text", text: text.slice(last, m.index) });
    out.push({ type: "term", words: m[1] as string, slug: m[2] as string });
    last = m.index + m[0].length;
  }
  if (last < text.length) out.push({ type: "text", text: text.slice(last) });
  return out;
}

/**
 * Copy rendered for the page. `cite="popover"` makes cited values open the citation popover (reading text);
 * `"plain"` cites them without one (headings). Glossary terms inside a term's words are not nested.
 */
export function CopyText({ text, cite = "popover" }: { text: string; cite?: "popover" | "plain" }): ReactNode {
  return copyParts(text).map((part, i) => {
    const key = `${i}-${part.type}`;
    return part.type === "term" ? (
      <GlossaryTerm key={key} slug={part.slug}>
        {part.words}
      </GlossaryTerm>
    ) : (
      <RefText key={key} text={part.text} cite={cite} />
    );
  });
}

/** Copy as plain text: references filled in, glossary terms as their words. */
export function copyPlainText(text: string, data: PageDataResolver): string {
  return copyParts(text)
    .map((p) => (p.type === "term" ? p.words : data.plainText(p.text)))
    .join("");
}

/**
 * Copy as Markdown: references filled in and escaped. Glossary terms are their words: the Markdown hooks don't see
 * the glossary, and a link to an entry that may not exist would be worse than none.
 */
export function copyMarkdown(text: string, data: PageDataResolver): string {
  return escapeMarkdown(copyPlainText(text, data));
}

export type Block =
  | { type: "paragraph"; text: string }
  | { type: "heading"; text: string }
  | { type: "list"; items: string[] };

const HEADING = /^##\s+/;
const ITEM = /^-\s+/;

/** The writer's blocks, split on blank lines and classified. Empty blocks are dropped. */
export function parseBlocks(blocks: readonly string[]): Block[] {
  return blocks
    .flatMap((b) => b.replace(/\r\n?/g, "\n").split(/\n[ \t]*\n/))
    .map((b) => b.trim())
    .filter((b) => b !== "")
    .map((b): Block => {
      if (HEADING.test(b)) return { type: "heading", text: b.replace(HEADING, "").replace(/\s*\n\s*/g, " ") };
      const lines = b.split("\n").map((l) => l.trim());
      if (lines.every((l) => ITEM.test(l))) return { type: "list", items: lines.map((l) => l.replace(ITEM, "")) };
      return { type: "paragraph", text: lines.join(" ") };
    });
}

/** A heading's id: its words, lower case, hyphenated ("section-why-sales-fell"). Stable across renders. */
export function headingId(text: string, data: PageDataResolver): string {
  const words = copyPlainText(text, data)
    .toLowerCase()
    .normalize("NFKD")
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return `section-${words.slice(0, 60).replace(/-+$/, "") || "untitled"}`;
}
