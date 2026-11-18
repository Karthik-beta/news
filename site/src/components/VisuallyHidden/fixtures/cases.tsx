import type { FixtureCase } from "../../contract";
import { VisuallyHidden } from "../VisuallyHidden";

export const cases: FixtureCase[] = [
  {
    name: "hidden text completes a sentence",
    render: () => (
      <p className="text-label">
        Net sales <VisuallyHidden>in fiscal 2024 were</VisuallyHidden> $391.0bn
      </p>
    ),
  },
  {
    name: "focusable (skip link)",
    render: () => (
      <VisuallyHidden focusable>
        <a href="#main">Skip to content</a>
      </VisuallyHidden>
    ),
  },
];
