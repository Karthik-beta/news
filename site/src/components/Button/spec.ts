import { definePrimitive } from "../contract";

export default definePrimitive({
  name: "Button",
  purpose: "Interface button: primary (the one form submit), secondary, or text; a native <button>, disabled at 40%.",
  example: `<Button variant="primary" type="submit">Send report</Button>`,
});
