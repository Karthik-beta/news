import { definePrimitive } from "../contract";

export default definePrimitive({
  name: "TextArea",
  purpose: "Multi-line form field: the TextField label, hint, markers and error, resizable in height only.",
  example: `<TextArea id="report-what" name="what" label="What is wrong?" required />`,
});
