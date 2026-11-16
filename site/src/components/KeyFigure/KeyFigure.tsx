// A key figure (design/components.md, "Key figures"): a 1px ink rule on top; the label in ink-2; the value as a
// large figure with a dotted underline that opens its citation popover; the change in words, never coloured; an
// optional 120×36 sparkline to the right of the value. Inside KeyFigures it is one item of the row's list; on
// its own it is a row of one with its own source line. Static: the popover is native, so it ships no JavaScript.
import type { ComponentRenderer } from "@openuidev/react-lang";
import { createContext, type ReactNode, useContext } from "react";
import { usePageData } from "../../renderer/data/context";
import type { CitedSource, PageDataResolver } from "../../renderer/data/resolve";
import { SourceLine } from "../ChartFigure/ChartFigure";
import { sourcesOf } from "../ChartFigure/model";
import { RefText } from "../SourceRef/RefText";
import { SourceRef } from "../SourceRef/SourceRef";
import { Sparkline } from "../Sparkline/Sparkline";
import { changeText } from "./change";

export interface KeyFigureProps {
  label: string;
  value: string;
  change?: string | null;
  against?: string | null;
  trend?: string | null;
}

/** True inside a KeyFigures row, which provides the list and the source line. */
const InRow = createContext(false);

export function KeyFiguresRow({ children }: { children: ReactNode }) {
  return <InRow.Provider value={true}>{children}</InRow.Provider>;
}

/** The change in words ("Up 2.0% on fiscal 2023"), or null when there is none or it did not resolve. */
export function changeWords(data: PageDataResolver, props: KeyFigureProps): string | null {
  if (!props.change) return null;
  const v = data.value(props.change);
  if (!v.ok) return null;
  const against = props.against ? (data.period(props.against)?.label ?? undefined) : undefined;
  return changeText(v, "words", against);
}

/** Every source behind a figure: its value, its change and its trend. */
export function figureSources(data: PageDataResolver, props: KeyFigureProps): CitedSource[] {
  const lists: CitedSource[][] = [];
  const v = data.value(props.value);
  if (v.ok) lists.push(v.sources);
  if (props.change) {
    const c = data.value(props.change);
    if (c.ok) lists.push(c.sources);
  }
  if (props.trend) {
    const t = data.series(props.trend);
    if (t.ok) lists.push(t.sources);
  }
  return sourcesOf(...lists);
}

/** One figure: label, value, change, as a group of the row's description list. */
export function KeyFigureItem(props: KeyFigureProps) {
  const data = usePageData();
  const change = changeWords(data, props);
  const trend = props.trend ? data.series(props.trend) : null;
  return (
    <div className="key-figure">
      <dt className="key-figure__label text-label">
        <RefText text={props.label} cite="plain" />
      </dt>
      <dd className="key-figure__value">
        <span className="key-figure__figure text-figure" data-ref={props.value}>
          <SourceRef dataRef={props.value} />
        </span>
        {trend?.ok ? <Sparkline series={trend} className="key-figure__sparkline" /> : null}
      </dd>
      {change ? (
        <dd className="key-figure__change" data-ref={props.change ?? undefined}>
          {change}
        </dd>
      ) : null}
    </div>
  );
}

export const KeyFigure: ComponentRenderer<KeyFigureProps> = ({ props }) => {
  const inRow = useContext(InRow);
  const data = usePageData();
  if (inRow) return <KeyFigureItem {...props} />;
  return (
    <div className="key-figures key-figures--n1">
      <dl className="key-figures__list">
        <KeyFigureItem {...props} />
      </dl>
      <SourceLine sources={figureSources(data, props)} className="key-figures__source" />
    </div>
  );
};
