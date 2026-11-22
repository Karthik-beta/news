import { definePrimitive } from "../contract";

export default definePrimitive({
  name: "TextField",
  purpose: "Form text input: label above, hint, 16px field, required or optional marker, error under the field.",
  example: `<TextField id="report-where" name="where" label="Where on the page?" required={false} />`,
});
