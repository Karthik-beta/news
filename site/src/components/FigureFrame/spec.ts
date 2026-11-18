import { definePrimitive } from "../contract";

export default definePrimitive({
  name: "FigureFrame",
  purpose: "The chart and table frame: brand rule and tag, title, subtitle, legend, plot, source line, actions.",
  example: `<FigureFrame id="chart" title="Services earns about twice the gross margin of products"
  subtitle="Gross margin as a share of net sales, fiscal 2020–2024, %" sources={series.sources}
  table={<table>…</table>} downloads={{ csv: "charts/chart.csv", json: "charts/chart.json" }}>
  <svg>…</svg>
</FigureFrame>`,
});
