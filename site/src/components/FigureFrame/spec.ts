import { definePrimitive } from "../contract";

export default definePrimitive({
  name: "FigureFrame",
  purpose: "The chart and table frame: brand rule and tag, title, subtitle, legend, plot, source line, actions.",
  example: `<FigureFrame id="chart" title="Greater China was the only region where sales fell"
  subtitle="Net sales by region, fiscal 2024, US$ bn" sources={series.sources}
  table={<table>…</table>} downloads={{ csv: "charts/chart.csv", json: "charts/chart.json" }}>
  <svg>…</svg>
</FigureFrame>`,
});
