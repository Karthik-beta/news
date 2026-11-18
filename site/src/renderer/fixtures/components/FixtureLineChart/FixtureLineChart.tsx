// A fixture-only line chart that exercises the renderer end to end: an sr_ reference, FigureFrame, the island,
// the data table, CSV/JSON downloads and Markdown. Not part of the page writer's library; the real line chart is
// built with the Charts I family (#11).
import type { ComponentRenderer } from "@openuidev/react-lang";
import { FigureFrame } from "../../../../components/FigureFrame/FigureFrame";
import { RefText } from "../../../../components/SourceRef/RefText";
import { compactNumber, formatMoney, formatNumber, formatQuantity } from "../../../../format";
import { decimalToString, parseDecimal, shiftDecimal } from "../../../../format/decimal";
import { SCALE_EXPONENT } from "../../../../format/number";
import { usePageData } from "../../../data/context";
import { MISSING_TEXT, type ResolvedSeries } from "../../../data/resolve";
import { downloadsFor } from "../../../exports";
import { Island } from "../../../islands/Island";
import FixtureLineChartPlot, { type FixtureLineChartPlotProps } from "./FixtureLineChartPlot.client";

export interface FixtureLineChartProps {
  title: string;
  subtitle: string;
  series: string;
}

type OkSeries = Extract<ResolvedSeries, { ok: true }>;

/** The value as a reader sees it: "$391.0bn", "2.4%". */
export function displayValue(series: OkSeries, value: string): string {
  const scale = series.scale ?? "units";
  return series.currency
    ? formatMoney(value, series.currency, { scale })
    : formatQuantity(value, series.unit, { scale });
}

/** "Nice" tick step for a range: 1, 2 or 5 times a power of ten. */
function niceStep(range: number, target = 4): number {
  const raw = range / target;
  const pow = 10 ** Math.floor(Math.log10(raw));
  const unit = raw / pow;
  return (unit <= 1 ? 1 : unit <= 2 ? 2 : unit <= 5 ? 5 : 10) * pow;
}

/** Geometry for the island: percentages for x and y, gridline ticks labelled in the chart's display unit. */
export function plotGeometry(series: OkSeries, summary: string): FixtureLineChartPlotProps {
  const values = series.points.map((p) => (p.value === null ? null : Number(p.value))); // geometry only
  const present = values.filter((v): v is number => v !== null);
  const max = Math.max(0, ...present);
  const min = Math.min(0, ...present);
  const step = niceStep(max - min || Math.abs(max) || 1);
  const top = Math.ceil(max / step) * step || step;
  const bottom = Math.floor(min / step) * step;
  const y = (v: number) => (1 - (v - bottom) / (top - bottom)) * 100;
  const n = series.points.length;

  // Tick labels are bare numbers in the display unit; the unit is in the subtitle (design/content-style.md).
  const biggest = present.length ? String(present.reduce((a, b) => (Math.abs(b) > Math.abs(a) ? b : a))) : "0";
  const scaleExp = SCALE_EXPONENT[series.scale ?? "units"];
  const displayExp = /^-?\d+(\.\d+)?$/.test(biggest)
    ? compactNumber(biggest, { scale: series.scale ?? "units" }).exponent
    : 0;
  const stepExp = Math.floor(Math.log10(step));
  const places = Math.max(0, -stepExp);
  const tickDecimals = Math.max(0, -(stepExp + scaleExp - displayExp));
  const tickLabel = (v: number) => {
    const text = v.toFixed(places).replace(/^-(?=0(\.0+)?$)/, "");
    const shown = decimalToString(shiftDecimal(parseDecimal(text), scaleExp - displayExp));
    return formatNumber(shown, { decimals: tickDecimals });
  };
  const ticks: FixtureLineChartPlotProps["ticks"] = [];
  for (let v = bottom; v <= top + step / 2; v += step) {
    const rounded = Math.round(v / step) * step;
    ticks.push({ y: y(rounded), label: tickLabel(rounded), baseline: rounded === 0 });
  }

  return {
    summary,
    seriesLabel: series.label,
    points: series.points.map((p, i) => ({
      label: p.period.label ?? "Unknown period",
      display: p.value === null ? null : displayValue(series, p.value),
      x: n === 1 ? 50 : (i / (n - 1)) * 100,
      y: values[i] === null || values[i] === undefined ? null : y(values[i] as number),
      mid: i > 0 && i < n - 1,
    })),
    ticks,
  };
}

/** The chart's data in a sentence: every value when there are 12 or fewer (design/content-style.md). */
export function chartSummary(title: string, series: OkSeries): string {
  const pts = series.points;
  const say = (p: (typeof pts)[number]) =>
    `${p.period.label ?? "unknown period"}, ${p.value === null ? MISSING_TEXT.toLowerCase() : displayValue(series, p.value)}`;
  if (pts.length <= 12) return `${title}. ${series.label}: ${pts.map(say).join("; ")}.`;
  const first = pts[0] as (typeof pts)[number];
  const lastPoint = pts[pts.length - 1] as (typeof pts)[number];
  return `${title}. ${series.label}, ${pts.length} periods, from ${say(first)} to ${say(lastPoint)}.`;
}

export const FixtureLineChart: ComponentRenderer<FixtureLineChartProps> = ({ props, statementId }) => {
  const data = usePageData();
  const id = statementId ?? "chart";
  const series = data.series(props.series);
  const title = <RefText text={props.title} cite="plain" />;
  const subtitle = <RefText text={props.subtitle} cite="plain" />;
  if (!series.ok) {
    return (
      <FigureFrame id={id} title={title} subtitle={subtitle}>
        <p className="fixture-chart-missing text-label">{MISSING_TEXT}</p>
      </FigureFrame>
    );
  }
  const plainTitle = data.plainText(props.title);
  const table = (
    <table aria-labelledby={`${id}-title`}>
      <thead>
        <tr>
          <th scope="col">Period</th>
          <th scope="col" className="is-number">
            {series.label}
          </th>
        </tr>
      </thead>
      <tbody>
        {series.points.map((p) => (
          <tr key={p.period.ref}>
            <th scope="row">{p.period.label ?? "Unknown period"}</th>
            <td className="is-number">{p.value === null ? MISSING_TEXT : displayValue(series, p.value)}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
  return (
    <FigureFrame
      id={id}
      title={title}
      subtitle={subtitle}
      sources={series.sources}
      table={table}
      downloads={downloadsFor(statementId, series.sources)}
    >
      <Island
        name="FixtureLineChartPlot"
        component={FixtureLineChartPlot}
        props={plotGeometry(series, chartSummary(plainTitle, series))}
      />
    </FigureFrame>
  );
};
