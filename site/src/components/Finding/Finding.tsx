// A finding (design/components.md, "Finding"): for science pages. A 2px ink rule on top; the finding stated in one
// sentence (Literata 600); the key number, if any, as a figure with its label; the method in one line (Fira,
// ink-2: "Peer-reviewed paper, 1,204 participants, randomised controlled trial"); the source type as a badge,
// chosen from sources.json, never from the writer's words; one source line. Several findings stack 24px apart.
// Static: 0 KB of JavaScript.
import type { ComponentRenderer } from "@openuidev/react-lang";
import { usePageData } from "../../renderer/data/context";
import type { CitedSource, PageDataResolver } from "../../renderer/data/resolve";
import { Badge, type BadgeKind, typeBadge } from "../Badge/Badge";
import { SourceLine } from "../ChartFigure/ChartFigure";
import { sourcesOf } from "../ChartFigure/model";
import { CopyText } from "../Prose/copy";
import { RefText } from "../SourceRef/RefText";
import { SourceRef } from "../SourceRef/SourceRef";
import { textSources } from "../Timeline/sources";

export interface FindingProps {
  text: string;
  figure?: string | null;
  label?: string | null;
  method?: string | null;
}

/** Every source the finding cites: its sentence, its figure and its method line. */
export function findingSources(data: PageDataResolver, props: FindingProps): CitedSource[] {
  const lists = [textSources(data, props.text)];
  if (props.figure) {
    const v = data.value(props.figure);
    if (v.ok) lists.push(v.sources);
  }
  if (props.method) lists.push(textSources(data, props.method));
  return sourcesOf(...lists);
}

/** The source-type badges of the cited sources ("Peer-reviewed", "Preprint", "Dataset"), each once. */
export const findingBadges = (sources: readonly CitedSource[]): BadgeKind[] => [
  ...new Set(sources.map((s) => typeBadge(s.source)).filter((b): b is BadgeKind => b !== null)),
];

/** The figure's label: the writer's, else the data point's own label. */
export function figureLabel(data: PageDataResolver, props: FindingProps): string | null {
  if (props.label) return data.plainText(props.label);
  if (!props.figure) return null;
  return data.value(props.figure).label;
}

export const Finding: ComponentRenderer<FindingProps> = ({ props }) => {
  const data = usePageData();
  const sources = findingSources(data, props);
  const badges = findingBadges(sources);
  const label = figureLabel(data, props);
  return (
    <div className="finding">
      <p className="finding__text">
        <CopyText text={props.text} />
      </p>
      {props.figure ? (
        <p className="finding__figure">
          <span className="finding__value text-figure" data-ref={props.figure}>
            <SourceRef dataRef={props.figure} />
          </span>
          {label ? (
            <span className="finding__label text-label">
              {props.label ? <RefText text={props.label} cite="plain" /> : label}
            </span>
          ) : null}
        </p>
      ) : null}
      {props.method ? (
        <p className="finding__method">
          <CopyText text={props.method} />
        </p>
      ) : null}
      {badges.length > 0 ? (
        <p className="finding__badges">
          {badges.map((b) => (
            <Badge key={b} kind={b} />
          ))}
        </p>
      ) : null}
      <SourceLine sources={sources} className="finding__source" />
    </div>
  );
};
