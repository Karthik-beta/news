import { definePrimitive } from "../contract";

export default definePrimitive({
  name: "DataLinks",
  purpose:
    "The downloads row: CSV, JSON and Markdown as underlined text links in ink-2; only the links given are shown.",
  example: `<DataLinks csv="charts/regions.csv" json="charts/regions.json" markdown="index.md" context="Net sales by region" />`,
});
