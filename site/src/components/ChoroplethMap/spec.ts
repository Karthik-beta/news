import { z } from "zod/v4";
import { MISSING_TEXT } from "../../renderer/data/resolve";
import { escapeMarkdown } from "../../renderer/markdown";
import { modelMarkdown, modelToChartTable } from "../ChartFigure/model";
import { defineNewsComponent } from "../contract";
import { valueRef } from "../Waterfall/spec";
import { ChoroplethMap, choroplethData } from "./ChoroplethMap";

export default defineNewsComponent({
  kind: "component",
  name: "ChoroplethMap",
  index: "ChoroplethMap — world map shaded by value — {region, label, dp_}[]",
  description:
    "Choropleth world map: each country shaded on the sequential blue ramp by its value, with the values ranked in a table (ranked bars on phones).",
  dataShape:
    'regions: 1–250 {region, label, value}: region the ISO 3166-1 alpha-3 code ("FRA"; Kosovo "KOS"), each once; label the short common name ("United States", "South Korea"); value one dp_… or dv_…, all in one unit (others are left out). Non-negative magnitudes only (rates, shares, levels); signed changes go in a bar chart. Use a map only when geography matters, best with 20 or more countries; otherwise a bar chart. Countries not listed, or with no value, are hatched as "No data". Nothing is highlighted.',
  example: `jobless = ChoroplethMap("Unemployment was highest in southern Africa", "Unemployment rate, 2024, %", [{region: "ZAF", label: "South Africa", value: "dp_imf_lur_zaf_2024"}, {region: "ESP", label: "Spain", value: "dp_imf_lur_esp_2024"}])`,
  props: z.object({
    title: z.string().describe("The takeaway, a sentence with no full stop"),
    subtitle: z.string().describe("What is shaded, the period and the unit"),
    regions: z
      .array(
        z.object({
          region: z
            .string()
            .regex(/^[A-Z]{3}$/)
            .describe(
              "ISO 3166-1 alpha-3 code; Natural Earth's non-ISO codes also work: KOS Kosovo, CYN Northern Cyprus, SOL Somaliland, KAS Siachen Glacier, ATC Ashmore and Cartier Islands, IOA Indian Ocean Territories",
            ),
          label: z.string().min(1).describe("The country's name, as text"),
          value: valueRef.describe("dp_… or dv_… of the country's value"),
        }),
      )
      .min(1)
      .max(250)
      .refine((r) => new Set(r.map((x) => x.region)).size === r.length, "Each region at most once"),
  }),
  component: ChoroplethMap,
  markdown: ({ props, data }) => {
    const { model } = choroplethData(props, data);
    return model ? modelMarkdown(model) : `### ${escapeMarkdown(data.plainText(props.title))}\n\n${MISSING_TEXT}.`;
  },
  csv: ({ props, data }) => {
    const { model } = choroplethData(props, data);
    return model ? modelToChartTable(model) : null;
  },
});
