import type { FixtureCase } from "../../contract";
import { Standfirst } from "../Standfirst";

export const cases: FixtureCase[] = [
  {
    name: "typical, with cited figures",
    render: () => (
      <Standfirst text="Total net sales rose by {dv_aapl_net_sales_growth_fy2024} on {pd_fy2023}, {cx_aapl_first_rise_2y}. The figures come from the company's annual reports." />
    ),
  },
  { name: "short", render: () => <Standfirst text="Prices held steady." /> },
  {
    name: "long, with a long URL",
    render: () => (
      <Standfirst text="The statistics office published the revision in its release notes at https://example.org/statistics/releases/2026/very-long-path-name-that-does-not-break-naturally/index.html alongside the tables, which changed the series for every month since the base year." />
    ),
  },
  { name: "missing value reads 'no data'", render: () => <Standfirst text="Revenue was {dp_edge_null} this year." /> },
];
