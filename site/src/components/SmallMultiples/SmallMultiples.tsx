// Small multiples (design/charts.md, "Small multiples"): many comparable series, one panel each, the same size and
// the same scales in every panel: one x axis (every panel's periods) and one value axis (every panel's values). Each
// panel is the line or column chart's own plot and island, given the shared scales; no new renderer. With nothing
// highlighted every panel is blue; naming one series makes its panel crimson (the neutral highlight when it fell,
// by the line rule) and the rest context. One frame, one data table, one CSV for all panels.
import type { ComponentRenderer } from "@openuidev/react-lang";
import type { CSSProperties } from "react";
import { usePageData } from "../../renderer/data/context";
import { MISSING_TEXT, type PageDataResolver } from "../../renderer/data/resolve";
import { Island } from "../../renderer/islands/Island";
import { ChartFigure, hasValues } from "../ChartFigure/ChartFigure";
import {
  type ColourRole,
  type OkSeries,
  type PeriodSlot,
  periodSlots,
  resolveSeries,
  sameAxis,
  seriesFell,
  seriesTable,
  subjectRole,
  type TableModel,
  valueFell,
} from "../ChartFigure/model";
import { columnPlot } from "../ColumnChart/ColumnChart";
import ColumnChartPlot, { type ColumnChartPlotProps } from "../ColumnChart/ColumnChartPlot.client";
import { linePlot } from "../LineChart/geometry";
import LineChartPlot, { type LineChartPlotProps } from "../LineChart/LineChartPlot.client";

export interface SmallMultiplesProps {
  title: string;
  subtitle: string;
  series: string[];
  /** "line" (the default) or "column": the chart each panel draws. */
  form?: "line" | "column" | null;
  /** The sr_… whose panel the title is about. */
  highlight?: string | null;
}

export interface SmallMultiplesData {
  series: OkSeries[];
  model: TableModel | null;
  /** The scales every panel shares: all panels' periods and all panels' values. */
  shared: { slots: PeriodSlot[]; values: string[] };
}

/** The series that share the first one's unit (never converted to match), their table and their shared scales. */
export function smallMultiplesData(props: SmallMultiplesProps, data: PageDataResolver): SmallMultiplesData {
  const resolved = resolveSeries(data, props.series);
  const first = resolved[0];
  const series = first ? resolved.filter((s) => sameAxis([first, s])) : [];
  const shared = {
    slots: periodSlots(series),
    values: series.flatMap((s) => s.points.map((p) => p.value).filter((v): v is string => v !== null)),
  };
  if (series.length === 0) return { series, model: null, shared };
  const model = seriesTable(series, { title: data.plainText(props.title), subtitle: data.plainText(props.subtitle) });
  return { series, model, shared };
}

/**
 * A panel's role: blue with nothing highlighted; else the subject or context. The subject fell by the line rule (its
 * last value below its first); a panel of columns also fell when any column is below zero, so no column below the
 * baseline is ever crimson (design/charts.md, "When the subject fell").
 */
export function panelRole(s: OkSeries, highlight: string | null | undefined, form: "line" | "column"): ColourRole {
  if (!highlight) return "cat-1";
  if (s.ref !== highlight) return "context";
  const values = s.points.map((p) => p.value);
  return subjectRole(seriesFell(values) || (form === "column" && values.some((v) => valueFell(v))));
}

export type Panel =
  | { key: string; label: string; form: "line"; plot: LineChartPlotProps }
  | { key: string; label: string; form: "column"; plot: ColumnChartPlotProps };

/** Each panel's plot, from the line or column chart's own geometry with the shared scales. */
export function smallMultiplesPanels(
  chart: SmallMultiplesData,
  props: SmallMultiplesProps,
  data: PageDataResolver,
): Panel[] {
  const highlight = props.highlight && chart.series.some((s) => s.ref === props.highlight) ? props.highlight : null;
  const panels = chart.series.map((s): Panel => {
    const form = props.form === "column" ? "column" : "line";
    const role = panelRole(s, highlight, form);
    // Each panel's accessible name is the chart's title and that panel's data.
    const model = seriesTable([s], { title: data.plainText(props.title), subtitle: "" });
    if (form === "column") {
      const plot = columnPlot(
        { series: [s], model },
        { title: s.label, subtitle: "", series: [s.ref], highlight: null },
        data,
        chart.shared,
      );
      const recolour = <T extends { role: string }>(x: T): T => ({ ...x, role });
      return {
        key: s.ref,
        label: s.label,
        form: "column",
        plot: {
          ...plot,
          bands: plot.bands.map((b) => ({ ...b, marks: b.marks.map(recolour), tooltip: b.tooltip.map(recolour) })),
        },
      };
    }
    // The subject's panel comes from the line chart's own highlight rule; context panels are recoloured, thin.
    const plot = linePlot(
      { series: [s], model },
      { curve: "linear", highlight: role === "context" ? null : highlight, shared: chart.shared },
      data,
    );
    if (role !== "context") return { key: s.ref, label: s.label, form: "line", plot };
    return {
      key: s.ref,
      label: s.label,
      form: "line",
      plot: {
        ...plot,
        series: plot.series.map((g) => ({ ...g, role, thin: true })),
        stops: plot.stops.map((st) => ({ ...st, tooltip: st.tooltip.map((l) => ({ ...l, role })) })),
      },
    };
  });
  // End labels differ in length from panel to panel; every line panel keeps the room of the widest and its x labels,
  // so the plots are the same width and their periods line up.
  const lines = panels.flatMap((p) => (p.form === "line" ? [p.plot] : []));
  const widest = lines.reduce<LineChartPlotProps | null>(
    (a, b) => (a && Number.parseFloat(a.endRoom.wide) >= Number.parseFloat(b.endRoom.wide) ? a : b),
    null,
  );
  if (widest) {
    for (const p of lines) {
      p.endRoom = widest.endRoom;
      p.xLabels = widest.xLabels;
    }
  }
  // A panel is at most a phone's plot wide (two across a 680px column), so it thins its x labels as on a phone at
  // every width; the wide plan assumes the whole column and collides ("fiscal 2020fiscal 2021" at 1440).
  for (const p of panels) p.plot.xLabels = p.plot.xLabels.map((l) => ({ ...l, wide: l.phone }));
  return panels;
}

/** Columns per row: 2 on phones; 3 or 4 wider, chosen so the last row is as full as it can be. */
export function panelColumns(n: number): { mid: number; wide: number } {
  const mid = n <= 2 || n === 4 ? 2 : 3;
  const wide = n <= 3 ? n : n === 5 || n === 6 || n === 9 ? 3 : 4;
  return { mid, wide };
}

export const SmallMultiples: ComponentRenderer<SmallMultiplesProps> = ({ props, statementId }) => {
  const data = usePageData();
  const chart = smallMultiplesData(props, data);
  const panels = hasValues(chart.model) ? smallMultiplesPanels(chart, props, data) : [];
  const columns = panelColumns(panels.length);
  return (
    <ChartFigure statementId={statementId} title={props.title} subtitle={props.subtitle} model={chart.model}>
      {panels.length ? (
        <div
          className="small-multiples"
          style={{ "--small-multiples-mid": columns.mid, "--small-multiples-wide": columns.wide } as CSSProperties}
        >
          <ul className="small-multiples__grid">
            {panels.map((p) => (
              <li key={p.key} className={`small-multiples__panel${p.key === props.highlight ? " is-highlight" : ""}`}>
                <p className="small-multiples__name">{p.label}</p>
                {/* A series with no values draws no empty plot (and no tab stop of empty stops): it says so. */}
                {!chart.series.find((s) => s.ref === p.key)?.points.some((pt) => pt.value !== null) ? (
                  <p className="small-multiples__missing text-label">{MISSING_TEXT}</p>
                ) : p.form === "column" ? (
                  <Island name="ColumnChartPlot" component={ColumnChartPlot} props={p.plot} />
                ) : (
                  <Island name="LineChartPlot" component={LineChartPlot} props={p.plot} />
                )}
              </li>
            ))}
          </ul>
        </div>
      ) : null}
    </ChartFigure>
  );
};
