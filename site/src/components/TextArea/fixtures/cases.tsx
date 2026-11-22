import type { FixtureCase } from "../../contract";
import { TextArea } from "../TextArea";

export const cases: FixtureCase[] = [
  { name: "required", render: () => <TextArea id="fixture-ta-required" label="What is wrong?" required /> },
  {
    name: "filled in, with a hint",
    render: () => (
      <TextArea
        id="fixture-ta-filled"
        label="What is wrong?"
        hint="Say which figure looks wrong and what the source says."
        required
        value={
          "The chart says net sales were $391.0bn.\nThe 10-K, page 21, says $391,035 million, which rounds the same way."
        }
      />
    ),
  },
  {
    name: "error",
    render: () => (
      <TextArea id="fixture-ta-error" label="What is wrong?" required rows={3} error="Say what is wrong." />
    ),
  },
];
