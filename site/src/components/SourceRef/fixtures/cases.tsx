import type { FixtureCase } from "../../contract";
import { SourceRef } from "../SourceRef";

// Rendered against the fixture bundle in src/renderer/fixtures/bundles/edge-cases/ (the gallery page provides it).
export const cases: FixtureCase[] = [
  {
    name: "money, compact",
    render: () => (
      <p className="text-body">
        Net sales were <SourceRef dataRef="dp_aapl_net_sales_fy2024" /> in fiscal 2024.
      </p>
    ),
  },
  {
    name: "money, as reported",
    render: () => (
      <p className="text-body">
        <SourceRef dataRef="dp_aapl_net_sales_fy2024" style="as-reported" />
      </p>
    ),
  },
  {
    name: "derived value (calculated by News)",
    render: () => (
      <p className="text-body">
        Up <SourceRef dataRef="dv_aapl_net_sales_growth_fy2024" /> on fiscal 2023.
      </p>
    ),
  },
  { name: "percentage", render: () => <SourceRef dataRef="dp_edge_percent" className="text-figure" /> },
  { name: "zero", render: () => <SourceRef dataRef="dp_edge_zero" className="text-figure" /> },
  { name: "negative", render: () => <SourceRef dataRef="dp_edge_negative" className="text-figure" /> },
  { name: "very large", render: () => <SourceRef dataRef="dp_edge_very_large" className="text-figure" /> },
  { name: "very small", render: () => <SourceRef dataRef="dp_edge_very_small" className="text-figure" /> },
  { name: "rupees", render: () => <SourceRef dataRef="dp_edge_inr" className="text-figure" /> },
  { name: "no value published (null)", render: () => <SourceRef dataRef="dp_edge_null" className="text-figure" /> },
  { name: "unknown reference", render: () => <SourceRef dataRef="dp_does_not_exist" className="text-figure" /> },
  // Added by the tester (news-component-testing): positions near the edges, and inside a heading.
  {
    name: "cited figure at the end of a right-aligned line",
    render: () => (
      <p className="text-body fixture-align-end">
        Net sales were <SourceRef dataRef="dp_aapl_net_sales_fy2024" />
      </p>
    ),
  },
  {
    name: "cited figure inside a heading",
    render: () => (
      <h3 className="text-headline-s">
        Sales reached <SourceRef dataRef="dp_aapl_net_sales_fy2024" />
      </h3>
    ),
  },
];
