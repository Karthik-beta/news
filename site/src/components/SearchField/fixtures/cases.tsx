import type { FixtureCase } from "../../contract";
import { SECTIONS } from "../../Masthead/Masthead";
import { REGIONS } from "../../RegionFilter/RegionFilter";
import { Select } from "../../Select/Select";
import { SearchField } from "../SearchField";

// The search page is not built yet, so the fixtures search the home page and the forms resolve in the fixture build.
// Several fields share this page, so each names its landmark differently; a real page has one, "Search pages".
const any = (label: string) => ({ value: "", label });
const filters = (key: string) => (
  <>
    <Select
      id={`${key}-category`}
      name="category"
      label="Category"
      options={[any("All categories"), ...SECTIONS.map((s) => ({ value: s.label.toLowerCase(), label: s.label }))]}
    />
    <Select
      id={`${key}-region`}
      name="region"
      label="Region"
      options={REGIONS.map((r) => ({ value: r.slug === "all" ? "" : r.slug, label: r.label }))}
      value="india"
    />
    <Select
      id={`${key}-type`}
      name="type"
      label="Page type"
      options={[any("All page types"), { value: "earnings", label: "Earnings" }, { value: "policy", label: "Policy" }]}
    />
    <Select
      id={`${key}-date`}
      name="date"
      label="Date"
      options={[any("Any time"), { value: "month", label: "Past month" }, { value: "year", label: "Past year" }]}
    />
  </>
);

export const cases: FixtureCase[] = [
  { name: "empty", render: () => <SearchField action="/" id="fixture-search-empty" /> },
  {
    name: "with a query and the filter row",
    render: () => (
      <SearchField
        action="/"
        id="fixture-search-filters"
        label="Search pages, filters case"
        query="apple services revenue"
        filters={filters("fixture-search-filters")}
      />
    ),
  },
];
