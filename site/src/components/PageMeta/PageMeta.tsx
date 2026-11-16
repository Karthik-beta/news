// The byline and meta lines under the standfirst (design/components.md, "Byline and meta"): three short lines in
// Fira 13.5, secondary ink, 3px apart, plus a fourth in ink when the page has been updated.
//   1. "Built with Claude (AI) · Automated checks · Run by Karthik" (the line links to the methodology page,
//      the name to the author's site). The middle dots are part of the agreed wording and used nowhere else.
//   2. "Data as of 1 November 2024." (the latest retrieval date of the page's sources), plus a fiscal-period note.
//   3. Categories, entity and region as plain text links, 14px apart.
//   4. "Updated on 3 December 2024", linking to the change note in the sources panel.
// A page-template part. Dates come from the page record and sources.json and go through the shared formatter.
import { formatDate } from "../../format";
import type { SourceT } from "../../renderer/bundle/schema";
import { escapeMarkdown } from "../../renderer/markdown";
import type { NavLink } from "../Masthead/Masthead";

export const BYLINE = {
  tool: "Built with Claude (AI) · Automated checks",
  runBy: "Run by",
  author: "Karthik",
} as const;
export const BYLINE_TEXT = `${BYLINE.tool} · ${BYLINE.runBy} ${BYLINE.author}`;

/** The anchor of the change note in the sources panel. */
export const CHANGE_NOTE_ID = "change-note";

/** "Data as of" is the latest retrieval date of the page's sources (design/content-style.md), as YYYY-MM-DD. */
export function latestRetrieval(sources: readonly Pick<SourceT, "retrieved_at">[]): string | null {
  const dates = sources.map((s) => s.retrieved_at.slice(0, 10)).sort();
  return dates.at(-1) ?? null;
}

export interface PageMetaProps {
  /** YYYY-MM-DD: latestRetrieval() of the sources the page cites. */
  dataAsOf: string | null;
  /** One short line when the fiscal period needs explaining ("Microsoft's fiscal year runs from July to June."). */
  fiscalNote?: string | null;
  /** Categories, then the entity, then the region, as links. */
  topics?: readonly NavLink[];
  /** YYYY-MM-DD, when the page has been revised. */
  updatedOn?: string | null;
  methodologyHref?: string;
  authorHref?: string;
  changeNoteHref?: string;
}

const sentence = (s: string) => (/[.!?]$/.test(s.trim()) ? s.trim() : `${s.trim()}.`);

export function PageMeta({
  dataAsOf,
  fiscalNote,
  topics = [],
  updatedOn,
  methodologyHref = "/methodology/",
  authorHref = "https://karthik.nz/",
  changeNoteHref = `#${CHANGE_NOTE_ID}`,
}: PageMetaProps) {
  return (
    <div className="page-meta">
      <p className="page-meta__line">
        <a href={methodologyHref}>{BYLINE.tool}</a> · {BYLINE.runBy} <a href={authorHref}>{BYLINE.author}</a>
      </p>
      <p className="page-meta__line">
        {dataAsOf ? (
          <>
            Data as of <time dateTime={dataAsOf}>{formatDate(dataAsOf)}</time>.
          </>
        ) : (
          "Data date not available."
        )}
        {fiscalNote?.trim() ? ` ${sentence(fiscalNote)}` : null}
      </p>
      {topics.length > 0 ? (
        <ul className="page-meta__line page-meta__topics" aria-label="Topics">
          {topics.map((t) => (
            <li key={t.href}>
              <a href={t.href}>{t.label}</a>
            </li>
          ))}
        </ul>
      ) : null}
      {updatedOn ? (
        <p className="page-meta__line page-meta__updated">
          <a href={changeNoteHref}>
            Updated on <time dateTime={updatedOn}>{formatDate(updatedOn)}</time>
          </a>
        </p>
      ) : null}
    </div>
  );
}

/** The meta lines for index.md, under the standfirst. */
export function pageMetaMarkdown(props: PageMetaProps): string {
  const lines = [
    escapeMarkdown(BYLINE_TEXT),
    [
      props.dataAsOf ? `Data as of ${formatDate(props.dataAsOf)}.` : "Data date not available.",
      props.fiscalNote?.trim() ? escapeMarkdown(sentence(props.fiscalNote)) : "",
    ]
      .filter(Boolean)
      .join(" "),
    (props.topics ?? []).map((t) => escapeMarkdown(t.label)).join(", "),
    props.updatedOn ? `Updated on ${formatDate(props.updatedOn)}` : "",
  ];
  // A backslash at the end of a line is a Markdown hard break that survives whitespace trimming.
  return lines.filter(Boolean).join("\\\n");
}
