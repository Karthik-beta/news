import { usePageData } from "../../../renderer/data/context";
import type { FixtureCase } from "../../contract";
import { Sparkline } from "../Sparkline";

// Rendered against the merged fixture bundles (the gallery page provides them).
function Trend({ refName }: { refName: string }) {
  const s = usePageData().series(refName);
  return s.ok ? <Sparkline series={s} /> : <span className="text-label">No data</span>;
}

export const cases: FixtureCase[] = [
  { name: "five fiscal years", render: () => <Trend refName="sr_aapl_net_sales_5y" /> },
  { name: "rising percentages", render: () => <Trend refName="sr_aapl_gm_services" /> },
  { name: "negative values", render: () => <Trend refName="sr_fx_negative" /> },
  { name: "gaps break the line", render: () => <Trend refName="sr_fx_gaps" /> },
  { name: "sixty points", render: () => <Trend refName="sr_fx_long" /> },
  { name: "a single point", render: () => <Trend refName="sr_fx_single" /> },
  { name: "all values equal to zero or missing", render: () => <Trend refName="sr_fx_allnull" /> },
  { name: "unknown series", render: () => <Trend refName="sr_fx_not_in_data" /> },
];
