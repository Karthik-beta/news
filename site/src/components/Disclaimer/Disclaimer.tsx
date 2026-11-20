// The disclaimer (design/components.md, "Disclaimer"): "Not investment advice." or "Not medical advice." in Fira 13,
// tertiary ink, at the end of the sources panel. Never a banner or a box. A page-template part: the page type
// decides which one (finance and economy pages, health science pages); other pages have none.
export type DisclaimerKind = "investment" | "medical";

export const DISCLAIMER_TEXT: Record<DisclaimerKind, string> = {
  investment: "Not investment advice.",
  medical: "Not medical advice.",
};

export function Disclaimer({ kind }: { kind: DisclaimerKind }) {
  return <p className="disclaimer">{DISCLAIMER_TEXT[kind]}</p>;
}
