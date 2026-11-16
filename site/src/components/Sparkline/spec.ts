import { definePrimitive } from "../contract";

export default definePrimitive({
  name: "Sparkline",
  purpose:
    "A 120×36 static trend line from an sr_ series, the latest point as a crimson dot, for key figures, lists and cards.",
  example: `const trend = usePageData().series("sr_aapl_net_sales_5y");
{trend.ok ? <Sparkline series={trend} /> : null}`,
});
