import { definePrimitive } from "../contract";

export default definePrimitive({
  name: "LatestList",
  purpose:
    "Page-template part (home): the newest 8 to 10 pages, automatic, each a time or date, fly title and headline.",
  example: `<LatestList pageIds={latest.map((e) => e.page_id)} now={renderedAt} />`,
});
