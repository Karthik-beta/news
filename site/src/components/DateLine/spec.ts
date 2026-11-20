import { definePrimitive } from "../contract";

export default definePrimitive({
  name: "DateLine",
  purpose: "Page-template part (home): the date with its weekday, the region filter at the right, a hairline under.",
  example: `<DateLine date="2024-11-06"><RegionFilter selected="all" /></DateLine>`,
});
