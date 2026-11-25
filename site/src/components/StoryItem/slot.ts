// A home page slot, as the stored curation names it (contracts/cloud/curation.v1.schema.json, "slot"): a page ID
// and, at most, one figure or chart of that page to show. Nothing else. There is no field for text: the slot shows
// the page record's own fly title, headline and summary, so an updated page updates its slot and a curator can
// never put words on the home page (design/layout.md, "Curated stories use their page's own words").
import { z } from "zod/v4";

/** The page ID pattern of contracts/cloud/common.schema.json. */
export const PAGE_ID = /^[a-z0-9](?:[a-z0-9-]{0,118}[a-z0-9])?$/;
/** A figure a slot can show: a dp_ or dv_ value in the page's data.json. */
export const SLOT_FIGURE = /^(dp|dv)_[a-z0-9]+(_[a-z0-9]+)*$/;
/** A chart ID: the chart's statement name, also its download's file name (charts/<id>.csv). */
export const CHART_ID = /^[A-Za-z][A-Za-z0-9-]{0,63}$/;

export const homeSlotProps = z
  .object({
    pageId: z.string().regex(PAGE_ID).describe("The page to show"),
    figure: z.string().regex(SLOT_FIGURE).nullable().optional().describe("One of the page's key figures"),
    chart: z.string().regex(CHART_ID).nullable().optional().describe("One of the page's charts, as a mini chart"),
  })
  .strict()
  .refine((s) => !(s.figure && s.chart), { message: "A slot shows a figure or a chart, not both" });

export type HomeSlot = z.infer<typeof homeSlotProps>;
