/** The four rules of design/layout.md ("Rules (lines)"). Nothing else draws lines between content. */
export type RuleKind = "hairline" | "ink" | "section" | "brand";

/**
 * Classes that put a rule on top of any element: `<figure className={ruleTop("brand")}>`. Used where a separate
 * rule element would break structure (a figure's figcaption must be its first child).
 */
export const ruleTop = (kind: RuleKind) => `rule-top rule-top--${kind}`;

/**
 * A rule as its own element. Decorative, so hidden from assistive technology (design/content-style.md: "the chart
 * tag, rules"). The brand rule carries the 36 × 7px crimson tag at its left end.
 */
export function Rule({ kind = "hairline", className }: { kind?: RuleKind; className?: string }) {
  return <div className={`rule rule--${kind}${className ? ` ${className}` : ""}`} aria-hidden="true" />;
}
