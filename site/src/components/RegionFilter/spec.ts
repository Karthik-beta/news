import { definePrimitive } from "../contract";

export default definePrimitive({
  name: "RegionFilter",
  purpose: "Page-template part: region links; the selected one in ink, semibold, with a crimson underline.",
  example: `<RegionFilter selected="india" base="/finance/" />`,
});
