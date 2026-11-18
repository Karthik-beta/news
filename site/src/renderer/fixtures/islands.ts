// Island loaders for fixture pages: production islands plus the fixture line chart.
import { islands } from "../../components/islands.generated";
import type { IslandLoaders } from "../islands/types";

export const fixtureIslands: IslandLoaders = {
  ...islands,
  FixtureLineChartPlot: () => import("./components/FixtureLineChart/FixtureLineChartPlot.client"),
};
