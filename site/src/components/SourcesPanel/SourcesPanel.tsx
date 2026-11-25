// The sources panel at the end of every article (design/components.md, "Sources panel"): a 2px ink rule, the
// heading "Sources", and one item per source the page cites (primary sources first, then context such as news
// outlets): the title as a link, then a meta row with its badge, source type and publisher, licence and retrieval
// date. Then the actions row, the change note when the page was updated, the accuracy note and the disclaimer.
// Each item has the id `source-<src id>`, which chart source lines link to. A page-template part: the template
// passes exactly the sources the page cited (the resolver's usedSources). Static: 0 KB JS.
import { formatDate } from "../../format";
import type { SourceT } from "../../renderer/bundle/schema";
import type { CitedSource } from "../../renderer/data/resolve";
import { escapeMarkdown } from "../../renderer/markdown";
import { BADGES, Badge, type BadgeKind, roleBadge, typeBadge } from "../Badge/Badge";
import { DISCLAIMER_TEXT, Disclaimer, type DisclaimerKind } from "../Disclaimer/Disclaimer";
import { ACCURACY_NOTE } from "../Footer/Footer";
import type { NavLink } from "../Masthead/Masthead";
import { CHANGE_NOTE_ID } from "../PageMeta/PageMeta";

export const TYPE_LABEL: Record<SourceT["type"], string> = {
  filing: "Filing",
  dataset: "Dataset",
  press_release: "Press release",
  statement: "Statement",
  report: "Report",
  paper: "Paper",
  preprint: "Preprint",
  news: "News article",
  other: "Source",
};

export interface SourcesPanelLinks {
  report: string;
  markdown: string;
  json: string;
  methodology: string;
}

export const DEFAULT_LINKS: SourcesPanelLinks = {
  report: "/report/",
  markdown: "index.md",
  json: "data.json",
  methodology: "/methodology/",
};

export const actionLinks = (links: SourcesPanelLinks): NavLink[] => [
  { label: "Report an error", href: links.report },
  { label: "Read as Markdown", href: links.markdown },
  { label: "Page data (JSON)", href: links.json },
  { label: "How these pages are made", href: links.methodology },
];

export interface ChangeNote {
  /** YYYY-MM-DD */
  date: string;
  /** The one-line reason, from the page record. */
  text: string;
}

export interface SourcesPanelProps {
  /** The sources the page cites, in order of first use. */
  sources: readonly CitedSource[];
  changeNote?: ChangeNote | null;
  disclaimer?: DisclaimerKind | null;
  links?: Partial<SourcesPanelLinks>;
  /** Prefix for the panel's ids. Only for showing several panels on one page (fixture galleries). */
  idPrefix?: string;
}

/** Primary sources first, then context; otherwise the order the page cited them. */
export const orderSources = (sources: readonly CitedSource[]) => [
  ...sources.filter((s) => s.source.primary),
  ...sources.filter((s) => !s.source.primary),
];

/** The badges a source carries: Primary or Context, plus Preprint or Peer-reviewed for papers. */
export function sourceBadges(source: SourceT): BadgeKind[] {
  const type = typeBadge(source);
  return [roleBadge(source), ...(type === "preprint" || type === "peer-reviewed" ? [type] : [])];
}

const sentence = (s: string) => (/[.!?]$/.test(s.trim()) ? s.trim() : `${s.trim()}.`);

/** "Filing, Apple Inc. Public filing. Retrieved 1 November 2024." */
export const sourceMeta = (s: SourceT) =>
  `${sentence(`${TYPE_LABEL[s.type]}, ${s.publisher}`)} ${sentence(s.licence.name)} Retrieved ${formatDate(s.retrieved_at)}.`;

export const changeNoteText = (note: ChangeNote) => `Updated on ${formatDate(note.date)}. ${sentence(note.text)}`;

export function SourcesPanel({ sources, changeNote, disclaimer, links, idPrefix = "" }: SourcesPanelProps) {
  const all = { ...DEFAULT_LINKS, ...links };
  const headingId = `${idPrefix}sources`;
  const ordered = orderSources(sources);
  return (
    // A plain section, not a named region: the page has one, and its h2 already puts it in the outline.
    <section className="sources-panel">
      <h2 className="sources-panel__heading text-headline-l" id={headingId}>
        Sources
      </h2>
      {ordered.length > 0 ? (
        <ol className="sources-panel__list">
          {ordered.map(({ id, source }) => (
            <li key={id} id={`${idPrefix}source-${id}`} className="sources-panel__item">
              <p className="sources-panel__title">
                <a href={source.url}>{source.title}</a>
              </p>
              <p className="sources-panel__meta">
                {sourceBadges(source).map((kind) => (
                  <Badge key={kind} kind={kind} />
                ))}
                <span>{sourceMeta(source)}</span>
              </p>
            </li>
          ))}
        </ol>
      ) : (
        <p className="sources-panel__empty text-label">No sources.</p>
      )}
      <ul className="sources-panel__actions">
        {actionLinks(all).map((l) => (
          <li key={l.label}>
            <a href={l.href}>{l.label}</a>
          </li>
        ))}
      </ul>
      {changeNote ? (
        <p className="sources-panel__change" id={`${idPrefix}${CHANGE_NOTE_ID}`}>
          {changeNoteText(changeNote)}
        </p>
      ) : null}
      <div className="sources-panel__notes">
        <p className="sources-panel__accuracy">{ACCURACY_NOTE}</p>
        {disclaimer ? <Disclaimer kind={disclaimer} /> : null}
      </div>
    </section>
  );
}

/** The sources section of index.md: the same items, actions left out (index.md is itself the Markdown). */
export function sourcesPanelMarkdown({ sources, changeNote, disclaimer }: SourcesPanelProps): string {
  const ordered = orderSources(sources);
  const items = ordered.map(
    ({ source }) =>
      `- [${escapeMarkdown(source.title)}](${source.url}). ${sourceBadges(source)
        .map((k) => BADGES[k])
        .join(", ")}. ${escapeMarkdown(sourceMeta(source))}`,
  );
  return [
    "## Sources",
    items.length ? items.join("\n") : "No sources.",
    changeNote ? escapeMarkdown(changeNoteText(changeNote)) : "",
    escapeMarkdown(ACCURACY_NOTE),
    disclaimer ? escapeMarkdown(DISCLAIMER_TEXT[disclaimer]) : "",
  ]
    .filter(Boolean)
    .join("\n\n");
}
