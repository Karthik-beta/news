import { definePrimitive } from "../contract";

export default definePrimitive({
  name: "SourcePopover",
  purpose:
    "The citation popover for one cited value: exact value as reported, document, location, retrieval date, link.",
  example: `const v = usePageData().value("dp_aapl_net_sales_fy2024");
{v.ok ? <SourcePopover id="cite-sales" value={v} shown="$391.0bn" triggerClassName="source-ref" /> : null}`,
});
