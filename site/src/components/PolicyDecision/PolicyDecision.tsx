// A policy decision (design/components.md, "Decision"): for central-bank and government decisions. No box: a 2px
// ink rule on top; the decision as a figure ("4.50%") with its label and the change in words ("Unchanged", "Cut by
// 0.25 points"); up to three supporting facts in a row of small figures (vote, next meeting, the rate before);
// one source line; then the rate path as a StepLineChart, when the page has the history. The change is written
// in the unit the data states it in (points or basis points), never converted, and never coloured.
// Static, apart from the rate path's own island.
import type { ComponentRenderer, ElementNode } from "@openuidev/react-lang";
import { isNegative, isZeroValue } from "../../format";
import { usePageData } from "../../renderer/data/context";
import { type CitedSource, type PageDataResolver, type ResolvedValue, refKind } from "../../renderer/data/resolve";
import { SourceLine } from "../ChartFigure/ChartFigure";
import { sourcesOf } from "../ChartFigure/model";
import { changeSize } from "../KeyFigure/change";
import { RefText } from "../SourceRef/RefText";
import { SourceRef } from "../SourceRef/SourceRef";
import { textSources } from "../Timeline/sources";

export interface DecisionFact {
  /** What the fact is, as text ("Vote", "Next meeting"). */
  label: string;
  /** dp_, dv_, pd_, q_ or cx_ reference. */
  value: string;
}

export interface PolicyDecisionProps {
  label: string;
  rate: string;
  change?: string | null;
  facts?: DecisionFact[] | null;
  /** A StepLineChart element: the rate path. */
  path?: unknown;
}

type OkValue = Extract<ResolvedValue, { ok: true }>;

/** "Unchanged", "Cut by 0.25 points", "Raised by 50 basis points": the decision's change, in its own unit. */
export function decisionWords(v: OkValue): string {
  const absolute = v.value.startsWith("-") ? v.value.slice(1) : v.value;
  const size = changeSize(v)(absolute);
  if (isZeroValue(v.value) || !/[1-9]/.test(size)) return "Unchanged";
  return `${isNegative(v.value) ? "Cut" : "Raised"} by ${size}`;
}

/** The change in words, or null when there is none or it did not resolve. */
export function decisionChange(data: PageDataResolver, change: string | null | undefined): string | null {
  if (!change) return null;
  const v = data.value(change);
  return v.ok ? decisionWords(v) : null;
}

/** At most three facts, in the writer's order. */
export const decisionFacts = (props: PolicyDecisionProps): DecisionFact[] => (props.facts ?? []).slice(0, 3);

/** Every source behind the decision and its facts (not the rate path, which has its own source line). */
export function decisionSources(data: PageDataResolver, props: PolicyDecisionProps): CitedSource[] {
  const lists: CitedSource[][] = [];
  for (const ref of [props.rate, props.change]) {
    if (!ref) continue;
    const v = data.value(ref);
    if (v.ok) lists.push(v.sources);
  }
  for (const fact of decisionFacts(props)) lists.push(textSources(data, `{${fact.value}}`));
  return sourcesOf(...lists);
}

export const isStepLine = (v: unknown): v is ElementNode =>
  typeof v === "object" &&
  v !== null &&
  (v as ElementNode).type === "element" &&
  (v as ElementNode).typeName === "StepLineChart";

export const PolicyDecision: ComponentRenderer<PolicyDecisionProps> = ({ props, renderNode }) => {
  const data = usePageData();
  const change = decisionChange(data, props.change);
  const facts = decisionFacts(props);
  return (
    <div className="policy-decision">
      <dl className="policy-decision__decision">
        <dt className="policy-decision__label text-label">
          <RefText text={props.label} cite="plain" />
        </dt>
        <dd className="policy-decision__rate text-figure" data-ref={props.rate}>
          <SourceRef dataRef={props.rate} />
        </dd>
        {change ? (
          <dd className="policy-decision__change" data-ref={props.change ?? undefined}>
            {change}
          </dd>
        ) : null}
      </dl>
      {facts.length > 0 ? (
        <dl className={`policy-decision__facts policy-decision__facts--n${facts.length}`}>
          {facts.map((fact) => (
            <div key={`${fact.label}-${fact.value}`} className="policy-decision__fact">
              <dt className="policy-decision__fact-label text-label">
                <RefText text={fact.label} cite="plain" />
              </dt>
              <dd
                className={`policy-decision__fact-value policy-decision__fact-value--${refKind(fact.value) ?? "other"}`}
                data-ref={fact.value}
              >
                <RefText text={`{${fact.value}}`} />
              </dd>
            </div>
          ))}
        </dl>
      ) : null}
      <SourceLine sources={decisionSources(data, props)} className="policy-decision__source" />
      {isStepLine(props.path) ? <div className="policy-decision__path">{renderNode(props.path)}</div> : null}
    </div>
  );
};
