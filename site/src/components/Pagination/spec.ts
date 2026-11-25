import { definePrimitive } from "../contract";

export default definePrimitive({
  name: "Pagination",
  purpose:
    "Page-template part (hubs, search, latest): Newer and Older at the ends, page numbers between, current marked.",
  example: `<Pagination current={3} total={12} base="/finance/" />`,
});
