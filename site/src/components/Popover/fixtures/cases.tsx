import type { FixtureCase } from "../../contract";
import { Popover, Tooltip } from "../Popover";

export const cases: FixtureCase[] = [
  {
    name: "popover inside a sentence",
    render: () => (
      <p className="text-body">
        Net sales were{" "}
        <Popover id="fixture-popover-1" label="Source for $391.0bn" trigger="$391.0bn" triggerClassName="source-ref">
          <span className="popover__value">$391,035 million</span>
          <span className="popover__line">Apple Inc. Form 10-K, fiscal 2024</span>
          <span className="popover__meta">Retrieved 1 November 2024</span>
        </Popover>{" "}
        in fiscal 2024.
      </p>
    ),
  },
  {
    name: "long content wraps at 320px",
    render: () => (
      <p className="text-body">
        <Popover id="fixture-popover-2" label="Definition" trigger="seasonally adjusted annual rate">
          <span className="popover__line">
            A monthly or quarterly figure adjusted for regular seasonal patterns and scaled up to a full year, so that
            periods of different lengths and seasons can be compared. Pneumonoultramicroscopicsilicovolcanoconiosis.
          </span>
        </Popover>
      </p>
    ),
  },
  {
    name: "tooltip (island look)",
    render: () => (
      <div className="fixture-stage">
        <Tooltip x={120} y={96}>
          <span className="popover__value">$391.0bn</span>
          <span className="popover__line">Net sales, fiscal 2024</span>
          <span className="popover__meta">Form 10-K, fiscal 2024</span>
        </Tooltip>
      </div>
    ),
  },
  // Added by the tester (news-component-testing): the other tooltip placements.
  {
    name: "tooltip below, aligned to the end",
    render: () => (
      <div className="fixture-stage">
        <Tooltip x="100%" y={0} placement="below" align="end">
          <span className="popover__value">$391.0bn</span>
        </Tooltip>
      </div>
    ),
  },
  {
    name: "tooltip below, aligned to the start",
    render: () => (
      <div className="fixture-stage">
        <Tooltip x={0} y={0} placement="below" align="start">
          <span className="popover__value">−5.7%</span>
        </Tooltip>
      </div>
    ),
  },
];
