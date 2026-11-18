import { z } from "zod/v4";
import { defineNewsComponent } from "../../../../components/contract";
import { refPattern } from "../../../bundle/schema";
import { FixtureLineChart } from "./FixtureLineChart";

export default defineNewsComponent({
  kind: "component",
  name: "FixtureLineChart",
  index: "FixtureLineChart — fixture only: line chart island — sr_ series",
  description: "Fixture-only line chart for renderer tests: one series over time.",
  dataShape: "series: an sr_… series of one measure; title and subtitle may hold {ref} tokens.",
  example: `chart = FixtureLineChart("Sales grew again in {pd_fy2024}", "Net sales, US$ bn", "sr_aapl_net_sales_5y")`,
  props: z.object({
    title: z.string().describe("The takeaway, a sentence with no full stop"),
    subtitle: z.string().describe("What, when and the unit"),
    series: z.string().regex(refPattern("sr")).describe("sr_… reference"),
  }),
  component: FixtureLineChart,
  markdown: ({ props, data }) => {
    const series = data.series(props.series);
    const title = `### ${data.plainText(props.title)}`;
    if (!series.ok) return `${title}\n\nNo data.`;
    const rows = series.points.map(
      (p) => `| ${p.period.label ?? "Unknown period"} | ${p.value === null ? "No data" : p.value} |`,
    );
    return [
      title,
      "",
      data.plainText(props.subtitle),
      "",
      `| Period | ${series.label} (${series.unit}${series.scale && series.scale !== "units" ? `, ${series.scale}` : ""}) |`,
      "| --- | ---: |",
      ...rows,
    ].join("\n");
  },
  csv: ({ props, data }) => {
    const series = data.series(props.series);
    if (!series.ok) return null;
    return {
      title: data.plainText(props.title),
      subtitle: data.plainText(props.subtitle),
      columns: [
        { key: "period", label: "Period" },
        { key: "value", label: series.label },
      ],
      rows: series.points.map((p) => ({ period: p.period.label, value: p.value })),
      unit: series.unit,
      currency: series.currency,
      scale: series.scale,
      sources: series.sources.map((s) => s.id),
    };
  },
});
