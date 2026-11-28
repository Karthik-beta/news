// A source card (design/components.md, "Source card"): the one primary document a page rests on (a report, a paper,
// a filing), placed in the body so the reader can open it. A 1px ink rule on top; the badges from sources.json
// (Primary or Context, plus Preprint or Peer-reviewed); the document's title as a link in Literata 600 with the
// external-link icon; a meta line in Fira, ink-2: type, publisher and publication date. Every word comes from the
// source record, never from the writer. Showing it records the source as cited, so the sources panel lists it.
// Static: 0 KB JS.
import type { ComponentRenderer } from "@openuidev/react-lang";
import { formatDate } from "../../format";
import type { SourceT } from "../../renderer/bundle/schema";
import { usePageData } from "../../renderer/data/context";
import { MISSING_TEXT } from "../../renderer/data/resolve";
import { Badge } from "../Badge/Badge";
import { Icon } from "../Icon/Icon";
import { sourceBadges, TYPE_LABEL } from "../SourcesPanel/SourcesPanel";

export interface SourceCardProps {
  /** src_… the document, from sources.json. */
  source: string;
}

/** "Report, International Monetary Fund, published 22 October 2024" (no date when the source gives none). */
export const sourceCardMeta = (s: SourceT) =>
  [`${TYPE_LABEL[s.type]}, ${s.publisher}`, s.published_at ? `published ${formatDate(s.published_at)}` : null]
    .filter(Boolean)
    .join(", ");

export const SourceCard: ComponentRenderer<SourceCardProps> = ({ props }) => {
  const cited = usePageData().citeSource(props.source);
  if (!cited) {
    return (
      <p className="source-card source-card--missing text-label" data-source={props.source}>
        {MISSING_TEXT}
      </p>
    );
  }
  const { source } = cited;
  return (
    <div className="source-card" data-source={cited.id}>
      <p className="source-card__badges">
        {sourceBadges(source).map((kind) => (
          <Badge key={kind} kind={kind} />
        ))}
      </p>
      <p className="source-card__title">
        <a href={source.url}>
          {source.title} <Icon name="external" />
        </a>
      </p>
      <p className="source-card__meta">{sourceCardMeta(source)}</p>
    </div>
  );
};
