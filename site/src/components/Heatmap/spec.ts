import { z } from "zod/v4";
import { refPattern } from "../../renderer/bundle/schema";
import { MISSING_TEXT } from "../../renderer/data/resolve";
import { escapeMarkdown } from "../../renderer/markdown";
import { modelMarkdown, modelToChartTable } from "../ChartFigure/model";
import { defineNewsComponent } from "../contract";
import { Heatmap, heatmapData } from "./Heatmap";

export default defineNewsComponent({
  kind: "component",
  name: "Heatmap",
  index: "Heatmap — series by period, shaded by value — sr_[] 1-12",
  description:
    "Heatmap: one row per series, one column per period, each cell shaded on the sequential blue ramp by its value; values written when they fit.",
  dataShape:
    "series: 1–12 sr_… series sharing one unit (rows, named by their labels); their periods are the columns, best up to about 24; on phones keep heatmaps to about 12 periods, as more make cells too thin to read. Non-negative magnitudes only (levels, shares); signed changes go in a line or column chart. Colour is magnitude, so nothing is highlighted. Title and subtitle may hold {ref} tokens; the subtitle names the unit.",
  example: `margins = Heatmap("Services margins were about twice those of products every year", "Gross margin as a share of net sales, fiscal years, %", ["sr_aapl_gm_services", "sr_aapl_gm_products"])`,
  props: z.object({
    title: z.string().describe("The takeaway, a sentence with no full stop"),
    subtitle: z.string().describe("What is plotted, the period and the unit"),
    series: z
      .array(z.string().regex(refPattern("sr")))
      .min(1)
      .max(12)
      .describe("sr_… references, 1–12, one row each"),
  }),
  component: Heatmap,
  markdown: ({ props, data }) => {
    const { model } = heatmapData(props, data);
    return model ? modelMarkdown(model) : `### ${escapeMarkdown(data.plainText(props.title))}\n\n${MISSING_TEXT}.`;
  },
  csv: ({ props, data }) => {
    const { model } = heatmapData(props, data);
    return model ? modelToChartTable(model) : null;
  },
});
