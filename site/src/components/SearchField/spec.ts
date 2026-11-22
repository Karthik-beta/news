import { definePrimitive } from "../contract";

export default definePrimitive({
  name: "SearchField",
  purpose: "Page-template part: the search form (GET, query `q`), 48px field and Search button, filters below.",
  example: `<SearchField query="apple services" filters={<Select id="filter-region" name="region" label="Region" options={regions} />} />`,
});
