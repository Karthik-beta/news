import { usePageData } from "../../../renderer/data/context";
import type { FixtureCase } from "../../contract";
import { ChartFigure, ChartLegend } from "../ChartFigure";
import { resolveSeries, seriesTable } from "../model";

// Rendered against the merged fixture bundles (the gallery page provides them). The plot is a placeholder: the
// chart components put their islands there.
function Frame({ refs, title, subtitle }: { refs: string[]; title: string; subtitle: string }) {
  const data = usePageData();
  const series = resolveSeries(data, refs);
  const model = series.length ? seriesTable(series, { title: data.plainText(title), subtitle }) : null;
  return (
    <ChartFigure
      title={title}
      subtitle={subtitle}
      model={model}
      legend={series.map((s, i) => ({
        key: s.ref,
        label: s.label,
        role: i === 0 ? "highlight" : "context",
        shape: "line",
      }))}
    >
      <p className="text-label">The plot goes here.</p>
    </ChartFigure>
  );
}

export const cases: FixtureCase[] = [
  {
    name: "two series: legend, source, data table",
    render: () => (
      <Frame
        refs={["sr_aapl_gm_services", "sr_aapl_gm_products"]}
        title="Services earns about twice the gross margin of products"
        subtitle="Gross margin as a share of net sales, fiscal years, %"
      />
    ),
  },
  {
    name: "one series: no legend",
    render: () => <Frame refs={["sr_aapl_services_5y"]} title="Services grew every year" subtitle="US$ bn" />,
  },
  {
    name: "no data",
    render: () => <Frame refs={["sr_fx_not_in_data"]} title="An unknown series" subtitle="Nothing to plot" />,
  },
  {
    name: "legend keys: squares and lines, every colour role",
    render: () => (
      <ChartLegend
        items={(
          [
            "highlight",
            "comparison",
            "context",
            "neutral",
            "cat-1",
            "cat-2",
            "cat-3",
            "cat-4",
            "cat-5",
            "cat-6",
            "cat-7",
            "cat-8",
          ] as const
        ).map((role, i) => ({ key: role, label: role, role, shape: i % 2 ? "line" : "square" }))}
      />
    ),
  },
];
