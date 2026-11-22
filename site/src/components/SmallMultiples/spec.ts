import { z } from "zod/v4";
import { refPattern } from "../../renderer/bundle/schema";
import { MISSING_TEXT } from "../../renderer/data/resolve";
import { escapeMarkdown } from "../../renderer/markdown";
import { modelMarkdown, modelToChartTable } from "../ChartFigure/model";
import { defineNewsComponent } from "../contract";
import { SmallMultiples, smallMultiplesData } from "./SmallMultiples";

export default defineNewsComponent({
  kind: "component",
  name: "SmallMultiples",
  index: "SmallMultiples — a panel per series, shared scales — sr_[] 2-12",
  description:
    "Small multiples: one small line or column chart per series, all on the same x and value scales; a highlighted series' panel is crimson.",
  dataShape:
    "series: 2–12 sr_… series sharing one unit (each is a panel, named by its label); form: line (default) or column; highlight: the sr_… whose panel the title is about. Title and subtitle may hold {ref} tokens; the subtitle names the unit.",
  example: `margins = SmallMultiples("Services earns about twice the gross margin of products", "Gross margin as a share of net sales, fiscal years, %", ["sr_aapl_gm_services", "sr_aapl_gm_products"], "line", "sr_aapl_gm_services")`,
  props: z.object({
    title: z.string().describe("The takeaway, a sentence with no full stop"),
    subtitle: z.string().describe("What is plotted, the period and the unit"),
    series: z
      .array(z.string().regex(refPattern("sr")))
      .min(2)
      .max(12)
      .describe("sr_… references, 2–12, one panel each"),
    form: z.enum(["line", "column"]).nullable().optional().describe("The chart in each panel; line by default"),
    highlight: z
      .string()
      .regex(refPattern("sr"))
      .nullable()
      .optional()
      .describe("sr_… of the panel the title is about"),
  }),
  component: SmallMultiples,
  markdown: ({ props, data }) => {
    const { model } = smallMultiplesData(props, data);
    return model ? modelMarkdown(model) : `### ${escapeMarkdown(data.plainText(props.title))}\n\n${MISSING_TEXT}.`;
  },
  csv: ({ props, data }) => {
    const { model } = smallMultiplesData(props, data);
    return model ? modelToChartTable(model) : null;
  },
});
