import type { FixtureCase } from "../../contract";
import { RegionFilter } from "../RegionFilter";

// Hubs are not built yet, so the hub case filters the home page; every link resolves in the fixture build.
export const cases: FixtureCase[] = [
  { name: "all regions selected", render: () => <RegionFilter selected="all" /> },
  { name: "one region selected", render: () => <RegionFilter selected="north-america" /> },
  {
    name: "one region only",
    render: () => <RegionFilter selected="global" regions={[{ slug: "global", label: "Global" }]} />,
  },
  { name: "unknown selection (nothing marked)", render: () => <RegionFilter selected="antarctica" /> },
];
