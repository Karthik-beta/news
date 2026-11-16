// A source badge (design/components.md, "Badges"): Fira 600, a 1px border in the text's colour, 3px radius. Text
// first; colour only reinforces it. Primary is focus blue; Context (news outlets) tertiary ink; the source-type
// badges secondary ink. A page-template part: the sources panel and science findings choose the badge from
// sources.json (`primary`, `type`), never from page copy.
import type { SourceT } from "../../renderer/bundle/schema";

export const BADGES = {
  primary: "Primary",
  context: "Context",
  preprint: "Preprint",
  "peer-reviewed": "Peer-reviewed",
  dataset: "Dataset",
  "press-release": "Press release",
  filing: "Filing",
} as const;

export type BadgeKind = keyof typeof BADGES;

export function Badge({ kind }: { kind: BadgeKind }) {
  return <span className={`badge badge--${kind}`}>{BADGES[kind]}</span>;
}

/** The source-type badge for a source, when its type has one (filings, datasets, press releases, papers). */
export function typeBadge(source: Pick<SourceT, "type">): BadgeKind | null {
  switch (source.type) {
    case "filing":
      return "filing";
    case "dataset":
      return "dataset";
    case "press_release":
      return "press-release";
    case "preprint":
      return "preprint";
    case "paper":
      return "peer-reviewed";
    default:
      return null;
  }
}

/** Primary or Context: whether the page's figures come from it, or it is context such as a news outlet. */
export const roleBadge = (source: Pick<SourceT, "primary">): BadgeKind => (source.primary ? "primary" : "context");
