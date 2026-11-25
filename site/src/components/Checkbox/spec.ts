import { definePrimitive } from "../contract";

export default definePrimitive({
  name: "Checkbox",
  purpose: "Native 24px checkbox with its label beside it; the field's hint, markers and error under the label.",
  example: `<Checkbox id="search-primary" name="primary" label="Primary sources only" />`,
});
