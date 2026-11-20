import { definePrimitive } from "../contract";

export default definePrimitive({
  name: "Standfirst",
  purpose: "Page-template part: the one-or-two-sentence standfirst under the headline, its figures cited.",
  example: `<Standfirst text={head.summary} />`,
});
