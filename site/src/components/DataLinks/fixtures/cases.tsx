import type { FixtureCase } from "../../contract";
import { DataLinks } from "../DataLinks";

// Links point at files the fixture build writes (the charts bundle's page), so the link check can follow them.
const PAGE = "/fixtures/renderer/charts";

export const cases: FixtureCase[] = [
  {
    name: "CSV, JSON and Markdown",
    render: () => (
      <DataLinks
        csv={`${PAGE}/charts/regions.csv`}
        json={`${PAGE}/charts/regions.json`}
        markdown={`${PAGE}/index.md`}
        context="Net sales by region"
      />
    ),
  },
  { name: "Markdown only", render: () => <DataLinks markdown={`${PAGE}/index.md`} /> },
  { name: "nothing to download renders nothing", render: () => <DataLinks /> },
];
