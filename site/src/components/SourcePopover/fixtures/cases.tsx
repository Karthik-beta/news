import { usePageData } from "../../../renderer/data/context";
import { formatValue } from "../../../renderer/data/resolve";
import type { FixtureCase } from "../../contract";
import { SourcePopover } from "../SourcePopover";

// Rendered against the merged fixture bundles (the gallery page provides them).
function Cited({ refName, id }: { refName: string; id: string }) {
  const v = usePageData().value(refName);
  if (!v.ok) return <span className="text-label">No data</span>;
  return (
    <p className="text-body">
      <SourcePopover id={id} value={v} shown={formatValue(v)} triggerClassName="source-ref" />
    </p>
  );
}

export const cases: FixtureCase[] = [
  { name: "XBRL filing value", render: () => <Cited refName="dp_aapl_net_sales_fy2024" id="cite-gallery-xbrl" /> },
  { name: "PDF page and table", render: () => <Cited refName="dp_aapl_gm_services_fy2024" id="cite-gallery-pdf" /> },
  {
    name: "calculated by News",
    render: () => <Cited refName="dv_aapl_services_change_fy2024" id="cite-gallery-derived" />,
  },
  { name: "dataset series", render: () => <Cited refName="dp_us_cpi_2024_12" id="cite-gallery-json" /> },
  { name: "rupees", render: () => <Cited refName="dp_edge_inr" id="cite-gallery-inr" /> },
];
