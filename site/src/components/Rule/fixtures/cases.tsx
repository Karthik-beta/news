import type { FixtureCase } from "../../contract";
import { Rule, ruleTop } from "../Rule";

export const cases: FixtureCase[] = [
  ...(["hairline", "ink", "section", "brand"] as const).map((kind) => ({
    name: kind,
    render: () => <Rule kind={kind} />,
  })),
  {
    name: "brand rule on top of a block",
    render: () => (
      <div className={ruleTop("brand")}>
        <p className="text-chart-title">Greater China was the only region where sales fell</p>
      </div>
    ),
  },
];
