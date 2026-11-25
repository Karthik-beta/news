import type { FixtureCase } from "../../contract";
import { LoadingRegion } from "../LoadingRegion";

// Stand-in results (the search page and its result list come with the page templates); links go to the home page.
const results = (
  <ol className="text-body">
    <li>
      <a href="/?result=apple-fy2024">Apple's services business grew while iPhone sales held flat</a>
    </li>
    <li>
      <a href="/?result=fed-december-2024">The Federal Reserve lowered its policy rate in December</a>
    </li>
    <li>
      <a href="/?result=india-cpi">India's consumer price inflation eased in November</a>
    </li>
  </ol>
);

export const cases: FixtureCase[] = [
  { name: "results shown", render: () => <LoadingRegion busy={false}>{results}</LoadingRegion> },
  { name: "loading new results", render: () => <LoadingRegion busy>{results}</LoadingRegion> },
];
