import type { FixtureCase } from "../../contract";
import { Checkbox } from "../Checkbox";

export const cases: FixtureCase[] = [
  { name: "unchecked", render: () => <Checkbox id="fixture-cb-plain" label="Primary sources only" /> },
  { name: "checked", render: () => <Checkbox id="fixture-cb-checked" label="Primary sources only" checked /> },
  {
    name: "required, with a hint and an error",
    render: () => (
      <Checkbox
        id="fixture-cb-error"
        label="I checked the figure against the linked source"
        hint="The source link is under the chart or in the sources panel at the end of the page."
        required
        error="Check the figure against the linked source first."
      />
    ),
  },
];
