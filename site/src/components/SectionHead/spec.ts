import { definePrimitive } from "../contract";

export default definePrimitive({
  name: "SectionHead",
  purpose: "Page-template part (home): a 2px ink rule, the category's name and a link to its hub, from the taxonomy.",
  example: `<SectionHead name={category.name} href={category.path} />`,
});
