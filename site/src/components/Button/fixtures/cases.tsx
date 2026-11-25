import type { FixtureCase } from "../../contract";
import { Button } from "../Button";

export const cases: FixtureCase[] = [
  { name: "primary", render: () => <Button variant="primary">Send report</Button> },
  { name: "secondary", render: () => <Button>Load video from YouTube</Button> },
  { name: "text button", render: () => <Button variant="text">Download CSV</Button> },
  {
    name: "disabled, with its reason beside it",
    render: () => (
      <p className="text-label">
        <Button variant="primary" disabled aria-describedby="fixture-button-reason">
          Send report
        </Button>{" "}
        <span id="fixture-button-reason">Reports reopen at 00:00 UTC.</span>
      </p>
    ),
  },
  {
    name: "long label wraps on a phone",
    render: () => <Button>Load video from YouTube: Federal Reserve press conference, 18 September 2024</Button>,
  },
];
