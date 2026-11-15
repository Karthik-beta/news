import { definePrimitive } from "../contract";

export default definePrimitive({
  name: "SourceRef",
  purpose:
    "A cited figure from a dp_/dv_ reference, formatted by code, with the citation popover; 'No data' if missing.",
  example: `<p className="text-body">Net sales were <SourceRef dataRef="dp_aapl_net_sales_fy2024" /> in fiscal 2024.</p>`,
});
