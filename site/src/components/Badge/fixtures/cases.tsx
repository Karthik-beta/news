import type { FixtureCase } from "../../contract";
import { BADGES, Badge, type BadgeKind } from "../Badge";

export const cases: FixtureCase[] = [
  ...(Object.keys(BADGES) as BadgeKind[]).map((kind) => ({
    name: `${BADGES[kind]}`,
    render: () => <Badge kind={kind} />,
  })),
  {
    name: "in a line of text",
    render: () => (
      <p className="text-label">
        <Badge kind="primary" /> <Badge kind="preprint" /> Filing, SEC EDGAR, Public filing, Retrieved 1 November 2024
      </p>
    ),
  },
];
