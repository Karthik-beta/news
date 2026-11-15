import type { ComponentRenderer } from "@openuidev/react-lang";
import { SourceRef } from "../../../../components/SourceRef/SourceRef";
import { formatChange, formatPercent, formatQuantity } from "../../../../format";
import { usePageData } from "../../../data/context";

export interface FixtureFigureProps {
  label: string;
  value: string;
  change?: string | null;
  against?: string | null;
}

/** Change in words from a dv_ value: "Up 2.0% on fiscal 2023". Never coloured. */
export function useChangeWords(change: string | null | undefined, against: string | null | undefined): string | null {
  const data = usePageData();
  if (!change) return null;
  const v = data.value(change);
  if (!v.ok) return null;
  const period = against ? data.period(against)?.label : undefined;
  const size = (x: string) =>
    v.unit === "%" || v.unit.toLowerCase() === "percent"
      ? formatPercent(x, { sourceDecimals: v.sourceDecimals ?? undefined })
      : formatQuantity(x, v.unit, { sourceDecimals: v.sourceDecimals ?? undefined });
  return formatChange(v.value, size, { against: period });
}

/** Fixture-only key figure for renderer tests. The real KeyFigures component comes in #8. */
export const FixtureFigure: ComponentRenderer<FixtureFigureProps> = ({ props }) => {
  const change = useChangeWords(props.change, props.against);
  return (
    <dl className="fixture-figure">
      <dt className="text-label">{props.label}</dt>
      <dd className="text-figure">
        <SourceRef dataRef={props.value} />
      </dd>
      {change ? <dd className="fixture-figure__change text-label">{change}</dd> : null}
    </dl>
  );
};
