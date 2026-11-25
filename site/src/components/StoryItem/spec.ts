import { definePrimitive } from "../contract";

export default definePrimitive({
  name: "StoryItem",
  purpose:
    "Page-template part (home rows, hubs, search): one page's fly title, headline, figure, summary and meta, from its record by page ID.",
  example: `<StoryItem pageId={slot.pageId} figure={slot.figure} variant="home" />
<StoryList pageIds={hubPageIds} variant="hub" />`,
});
