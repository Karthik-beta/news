import { definePrimitive } from "../contract";

export default definePrimitive({
  name: "ChartFigure",
  purpose:
    "The data-aware chart wrapper: FigureFrame filled from one table model (legend, sources, data table, CSV, JSON, No data).",
  example: `<ChartFigure statementId="chart" title="Sales grew again in {pd_fy2024}" subtitle="Net sales, US$ bn"
  model={seriesTable([series], { title, subtitle })} legend={[]}>
  <Island name="ColumnChartPlot" component={ColumnChartPlot} props={plot} />
</ChartFigure>`,
});
