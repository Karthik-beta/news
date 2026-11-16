import type { FixtureCase } from "../../contract";
import { DataLinks } from "../DataLinks";

export const cases: FixtureCase[] = [
  {
    name: "CSV, JSON and Markdown",
    render: () => (
      <DataLinks
        csv="charts/regions.csv"
        json="charts/regions.json"
        markdown="index.md"
        context="Net sales by region"
      />
    ),
  },
  { name: "Markdown only", render: () => <DataLinks markdown="index.md" /> },
  { name: "nothing to download renders nothing", render: () => <DataLinks /> },
];
