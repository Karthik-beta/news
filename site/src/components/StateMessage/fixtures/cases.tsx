import type { FixtureCase } from "../../contract";
import { StateMessage } from "../StateMessage";

// Hubs and search are not built yet, so the action links go to the home page with a query string.
export const cases: FixtureCase[] = [
  {
    name: "empty state",
    render: () => (
      <StateMessage
        message="No pages match these filters."
        action={{ href: "/?state=clear-region", label: "Clear the region filter" }}
      />
    ),
  },
  {
    name: "error state",
    render: () => (
      <StateMessage
        message="Search is unavailable until 00:00 UTC because today's limit was reached."
        action={{ href: "/?state=categories", label: "Browse by category" }}
      />
    ),
  },
  { name: "message only", render: () => <StateMessage message="No pages in this section yet." /> },
];
