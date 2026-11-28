import { z } from "zod/v4";
import { refPattern } from "../../renderer/bundle/schema";
import { MISSING_TEXT } from "../../renderer/data/resolve";
import { escapeMarkdown } from "../../renderer/markdown";
import { modelMarkdown, modelToChartTable, periodSlots } from "../ChartFigure/model";
import { defineNewsComponent } from "../contract";
import { lineChartData, numberedNotes } from "./geometry";
import { LineChart } from "./LineChart";

const seriesRef = z.string().regex(refPattern("sr"));

export default defineNewsComponent({
  kind: "component",
  name: "LineChart",
  index: "LineChart — change over time — sr_[] 1-4, sr_ highlight, pd_ notes",
  description:
    "Line chart for change over time: up to four series on one axis, labelled at their ends; the highlight series is crimson, one comparison blue; up to three numbered notes.",
  dataShape:
    "series: 1–4 sr_… series sharing one unit (others are left out); highlight: the sr_… the title is about; comparison: one sr_… to compare against; fromZero: false lets the axis start near the data; notes: up to three {at: pd_…, text}, one per plotted period, text a few words with no digits or {ref}s. Title and subtitle may hold {ref} tokens; the subtitle names the unit.",
  example: `margins = LineChart("Services earns about twice the gross margin of products", "Gross margin as a share of net sales, fiscal years, %", ["sr_aapl_gm_services", "sr_aapl_gm_products"], "sr_aapl_gm_services", "sr_aapl_gm_products", null, [{at: "pd_fy2021", text: "Chip shortage"}])`,
  props: z.object({
    title: z.string().describe("The takeaway, a sentence with no full stop"),
    subtitle: z.string().describe("What is plotted, the period and the unit"),
    series: z.array(seriesRef).min(1).max(4).describe("sr_… references, 1–4"),
    highlight: seriesRef.nullable().optional().describe("sr_… the title is about (crimson)"),
    comparison: seriesRef.nullable().optional().describe("one sr_… to compare against (blue)"),
    fromZero: z.boolean().nullable().optional().describe("false: the axis starts near the data"),
    notes: z
      .array(
        z.object({
          at: z.string().regex(refPattern("pd")).describe("pd_… a plotted period"),
          // Words only: figures on a page come from data, so a note can hold no digits and no {ref} tokens.
          text: z
            .string()
            .min(1)
            .max(40)
            .regex(/^[^{}0-9]*$/)
            .describe("A few words, no digits"),
        }),
      )
      .max(3)
      .nullable()
      .optional()
      .describe("Up to three notes on the chart"),
  }),
  component: LineChart,
  markdown: ({ props, data }) => {
    const { series, model } = lineChartData(props.series, props, data);
    if (!model) return `### ${escapeMarkdown(data.plainText(props.title))}\n\n${MISSING_TEXT}.`;
    const notes = numberedNotes(props.notes ?? [], periodSlots(series)).map(
      (n) => `${n.number}. ${escapeMarkdown(`${n.period}: ${n.text}`)}`,
    );
    return notes.length
      ? `${modelMarkdown(model)}\n\nNotes on the chart:\n\n${notes.join("\n")}`
      : modelMarkdown(model);
  },
  csv: ({ props, data }) => {
    const { model } = lineChartData(props.series, props, data);
    return model ? modelToChartTable(model) : null;
  },
});
