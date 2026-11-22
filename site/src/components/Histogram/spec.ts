import { z } from "zod/v4";
import { MISSING_TEXT } from "../../renderer/data/resolve";
import { escapeMarkdown } from "../../renderer/markdown";
import { modelMarkdown, modelToChartTable } from "../ChartFigure/model";
import { defineNewsComponent } from "../contract";
import { valueRef } from "../Waterfall/spec";
import { Histogram, histogramData } from "./Histogram";

export default defineNewsComponent({
  kind: "component",
  name: "Histogram",
  index: "Histogram — values counted in ranges — {label, dp_}[] 2-200",
  description:
    "Histogram: counts how many of the given values fall in each round-number range; a highlighted item's range is crimson.",
  dataShape:
    "items: 2–200 {label, value, highlight?}: label text (may hold {pd_…}); value one dp_… or dv_…, all in one unit (others are left out); highlight marks the item the title is about. Ranges are chosen by code.",
  example: `inflation = Histogram("Inflation stayed between 2% and 4% all year", "Consumer prices, change on a year earlier, monthly readings in 2024, %", [{label: "{pd_2024_01}", value: "dp_us_cpi_2024_01"}, {label: "{pd_2024_02}", value: "dp_us_cpi_2024_02"}])`,
  props: z.object({
    title: z.string().describe("The takeaway, a sentence with no full stop"),
    subtitle: z.string().describe("What is counted, the period and the unit"),
    items: z
      .array(
        z.object({
          label: z.string().min(1).describe("The item's name; may hold {pd_…} tokens"),
          value: valueRef.describe("dp_… or dv_… of the item's value"),
          highlight: z.boolean().nullable().optional().describe("The item the title is about"),
        }),
      )
      .min(2)
      .max(200),
  }),
  component: Histogram,
  markdown: ({ props, data }) => {
    const { model } = histogramData(props, data);
    return model ? modelMarkdown(model) : `### ${escapeMarkdown(data.plainText(props.title))}\n\n${MISSING_TEXT}.`;
  },
  csv: ({ props, data }) => {
    const { model } = histogramData(props, data);
    return model ? modelToChartTable(model) : null;
  },
});
