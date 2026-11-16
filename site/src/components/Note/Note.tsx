// A commentary note in Claude's own words (design/components.md, "Note"; design/content-style.md, "Commentary
// notes"): the wash background, 16px 20px padding, no border or radius; a heading that says what it answers; the
// text; and an attribution line built by code from the sources the note cites, so a reader always knows the words
// are Claude's and the figures are the source's. Static: 0 KB JS.
import type { ComponentRenderer } from "@openuidev/react-lang";
import type { SourceT } from "../../renderer/bundle/schema";
import { usePageData } from "../../renderer/data/context";
import { type CitedSource, type PageDataResolver, refKind, refsInText } from "../../renderer/data/resolve";
import { CopyText, copyParts, parseBlocks } from "../Prose/copy";

export interface NoteProps {
  heading: string;
  paragraphs: string[];
}

const DOCUMENT_NOUN: Record<SourceT["type"], string> = {
  filing: "the filing",
  dataset: "the dataset",
  press_release: "the press release",
  statement: "the statement",
  report: "the report",
  paper: "the paper",
  preprint: "the preprint",
  news: "the article",
  other: "the source",
};

/** Sources behind the values and quotes the note cites, in order of first use. */
export function noteSources(paragraphs: readonly string[], data: PageDataResolver): CitedSource[] {
  const out = new Map<string, CitedSource>();
  const refs = paragraphs.flatMap((p) =>
    copyParts(p).flatMap((part) => (part.type === "text" ? refsInText(part.text) : [])),
  );
  for (const ref of refs) {
    const kind = refKind(ref);
    const cited =
      kind === "dp" || kind === "dv"
        ? (() => {
            const v = data.value(ref);
            return v.ok ? v.sources : [];
          })()
        : kind === "q"
          ? (data.quote(ref)?.sources ?? [])
          : [];
    for (const c of cited) if (!out.has(c.id)) out.set(c.id, c);
  }
  return [...out.values()];
}

/** "Context written by Claude from Form 10-K, fiscal 2024. Figures are from the filing." */
export function noteAttribution(sources: readonly CitedSource[]): string {
  if (sources.length === 0) return "Context written by Claude from the sources listed on this page.";
  const names = sources.map((s) => s.source.short);
  const list = names.length === 1 ? names[0] : `${names.slice(0, -1).join(", ")} and ${names.at(-1)}`;
  const noun = sources.length === 1 ? DOCUMENT_NOUN[(sources[0] as CitedSource).source.type] : "those sources";
  return `Context written by Claude from ${list}. Figures are from ${noun}.`;
}

export const Note: ComponentRenderer<NoteProps> = ({ props }) => {
  const data = usePageData();
  const blocks = parseBlocks(props.paragraphs).map((b) =>
    b.type === "paragraph" ? b.text : b.type === "heading" ? b.text : b.items.join(" "),
  );
  const sources = noteSources(blocks, data);
  return (
    <aside className="note">
      <h2 className="note__heading">
        <CopyText text={props.heading} cite="plain" />
      </h2>
      {blocks.map((text, i) => (
        // biome-ignore lint/suspicious/noArrayIndexKey: paragraphs are static copy in a fixed order
        <p key={i} className="note__text text-body-s">
          <CopyText text={text} />
        </p>
      ))}
      <p className="note__attribution text-source">{noteAttribution(sources)}</p>
    </aside>
  );
};
