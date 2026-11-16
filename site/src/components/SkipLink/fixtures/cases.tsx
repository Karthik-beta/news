import type { FixtureCase } from "../../contract";
import { SkipLink } from "../SkipLink";

export const cases: FixtureCase[] = [
  {
    name: "skip link (Tab to show it; it targets the page's main element)",
    render: () => (
      <div className="fixture-skip-link">
        <SkipLink target="main" />
      </div>
    ),
  },
];
