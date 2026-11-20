import type { FixtureCase } from "../../contract";
import { FlyTitle } from "../FlyTitle";

export const cases: FixtureCase[] = [
  { name: "typical", render: () => <FlyTitle text="Apple's annual results" /> },
  { name: "short: one word", render: () => <FlyTitle text="Space" /> },
  { name: "with a period reference", render: () => <FlyTitle text="Apple's results for {pd_fy2024}" /> },
  {
    name: "long, with a very long word",
    render: () => (
      <FlyTitle text="Pneumonoultramicroscopicsilicovolcanoconiosis research from the national institutes" />
    ),
  },
  { name: "linked, as in lists", render: () => <FlyTitle text="India's monetary policy" href="/?story=1" /> },
];
