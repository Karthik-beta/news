import type { FixtureCase } from "../../contract";
import { ICON_NAMES, Icon } from "../Icon";

export const cases: FixtureCase[] = [
  ...ICON_NAMES.map((name) => ({
    name,
    render: () => (
      <p className="text-ui">
        <Icon name={name} /> {name}
      </p>
    ),
  })),
  {
    name: "labelled, standing alone",
    render: () => <Icon name="search" label="Search" />,
  },
  {
    name: "inherits colour and size",
    render: () => (
      <p className="text-fly">
        Error <Icon name="error" />
      </p>
    ),
  },
];
