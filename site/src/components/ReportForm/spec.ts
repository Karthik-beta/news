import { definePrimitive } from "../contract";

export default definePrimitive({
  name: "ReportForm",
  purpose: "Page-template part: the report-an-error form (POST to `action`), its error summary and its sent state.",
  example: `<ReportForm action="/report/" pageUrl="/finance/apple-fy2024/" values={values} errors={reportErrors(values)} />`,
});
