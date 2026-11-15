import type { ComponentRenderer } from "@openuidev/react-lang";
import { RefText } from "../../../../components/SourceRef/RefText";

/** Fixture-only paragraph with {ref} tokens, for renderer tests. The real Prose component comes in #13. */
export const FixtureText: ComponentRenderer<{ text: string }> = ({ props }) => (
  <p className="text-body">
    <RefText text={props.text} />
  </p>
);
