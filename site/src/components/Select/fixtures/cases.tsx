import type { FixtureCase } from "../../contract";
import { REGIONS } from "../../RegionFilter/RegionFilter";
import { Select } from "../Select";

const regions = REGIONS.map((r) => ({ value: r.slug === "all" ? "" : r.slug, label: r.label }));

export const cases: FixtureCase[] = [
  {
    name: "a filter, nothing chosen",
    render: () => <Select id="fixture-select-plain" label="Region" options={regions} />,
  },
  {
    name: "a filter, one chosen",
    render: () => <Select id="fixture-select-chosen" label="Region" options={regions} value="india" />,
  },
  {
    name: "required, with a hint and an error",
    render: () => (
      <Select
        id="fixture-select-error"
        label="Page type"
        hint="Choose the kind of page the report is about."
        required
        options={[
          { value: "", label: "Choose a page type" },
          { value: "earnings", label: "Earnings" },
          { value: "policy", label: "Policy decision" },
        ]}
        error="Choose a page type."
      />
    ),
  },
];
