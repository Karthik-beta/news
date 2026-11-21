import type { FixtureCase } from "../../contract";
import { SectionHead } from "../SectionHead";

// Category hubs are not built yet; links point at the home page with a query, so the link check passes.
export const cases: FixtureCase[] = [
  { name: "with the hub link", render: () => <SectionHead name="Economy" href="/?section=economy" /> },
  { name: "no hub link", render: () => <SectionHead name="Science" /> },
  {
    name: "a long category name wraps the link under it",
    render: () => (
      <SectionHead
        name="Energy, commodities and international trade statistics"
        href="/?section=energy-commodities-trade"
      />
    ),
  },
];
