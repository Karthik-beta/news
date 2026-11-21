import { definePrimitive } from "../contract";

export default definePrimitive({
  name: "LeadStory",
  purpose:
    "Page-template part (home): the lead slot's page record by page ID, with one of its figures or charts under a brand rule.",
  example: `<LeadStory pageId={curation.slots.lead.page_id} chart={curation.slots.lead.chart_id} />`,
});
