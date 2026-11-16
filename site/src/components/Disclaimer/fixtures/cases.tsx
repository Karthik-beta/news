import type { FixtureCase } from "../../contract";
import { Disclaimer } from "../Disclaimer";

export const cases: FixtureCase[] = [
  { name: "finance and economy pages", render: () => <Disclaimer kind="investment" /> },
  { name: "health science pages", render: () => <Disclaimer kind="medical" /> },
];
