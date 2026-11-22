import type { FixtureCase } from "../../contract";
import { ReportForm, reportErrors } from "../ReportForm";

// The intake comes with issue #14, so the fixtures post to the home page. A real page has one form; the ids repeat
// here across cases, so only one case shows the fields.
const invalid = { what: "  ", where: "The services chart", email: "reader@example" };

export const cases: FixtureCase[] = [
  {
    name: "with errors after sending",
    render: () => (
      <ReportForm action="/" pageUrl="/?page=apple-fy2024" values={invalid} errors={reportErrors(invalid)} />
    ),
  },
  { name: "sent", render: () => <ReportForm action="/" pageUrl="/?page=apple-fy2024" sent /> },
];
