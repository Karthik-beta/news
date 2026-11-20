import type { FixtureCase } from "../../contract";
import { RegionFilter } from "../RegionFilter";

// Hubs are not built yet, so every case filters the home page and its links resolve in the fixture build. Several
// filters share this page, so each names its navigation differently; a real page has one, named "Regions".
export const cases: FixtureCase[] = [
  { name: "all regions selected", render: () => <RegionFilter selected="all" /> },
  {
    name: "one region selected",
    render: () => <RegionFilter selected="north-america" label="Regions, North America case" />,
  },
  {
    name: "one region only",
    render: () => (
      <RegionFilter
        selected="global"
        regions={[{ slug: "global", label: "Global" }]}
        label="Regions, one-region case"
      />
    ),
  },
  {
    name: "unknown selection (nothing marked)",
    render: () => <RegionFilter selected="antarctica" label="Regions, unknown-selection case" />,
  },
];
