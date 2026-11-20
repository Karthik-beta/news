import { definePrimitive } from "../contract";

export default definePrimitive({
  name: "FlyTitle",
  purpose: "Page-template part: the crimson two-to-four-word topic line above a headline, from the page's head copy.",
  example: `<FlyTitle text={head.flyTitle} />`,
});
