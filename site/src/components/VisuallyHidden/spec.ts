import { definePrimitive } from "../contract";

export default definePrimitive({
  name: "VisuallyHidden",
  purpose: "Text for screen readers only; with `focusable`, shown while focused (skip links).",
  example: `<VisuallyHidden>Source: </VisuallyHidden>`,
});
