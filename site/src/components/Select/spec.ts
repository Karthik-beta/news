import { definePrimitive } from "../contract";

export default definePrimitive({
  name: "Select",
  purpose: "Native drop-down in the field look: label above, hint, markers and error; the search filters use it.",
  example: `<Select id="filter-region" name="region" label="Region" options={[{ value: "", label: "All regions" }]} />`,
});
