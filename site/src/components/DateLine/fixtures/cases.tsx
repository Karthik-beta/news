import type { FixtureCase } from "../../contract";
import { RegionFilter } from "../../RegionFilter/RegionFilter";
import { DateLine } from "../DateLine";

export const cases: FixtureCase[] = [
  { name: "date alone", render: () => <DateLine date="2024-11-06" /> },
  {
    name: "longest weekday and month, with the region filter",
    render: () => (
      <DateLine date="2026-09-30">
        <RegionFilter selected="all" />
      </DateLine>
    ),
  },
  {
    name: "a region selected",
    render: () => (
      <DateLine date="2024-11-06">
        <RegionFilter selected="india" label="Regions, India case" />
      </DateLine>
    ),
  },
];
