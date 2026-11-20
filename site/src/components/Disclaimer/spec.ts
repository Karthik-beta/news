import { definePrimitive } from "../contract";

export default definePrimitive({
  name: "Disclaimer",
  purpose: "Page-template part: 'Not investment advice.' or 'Not medical advice.' at the end of the sources panel.",
  example: `<Disclaimer kind="investment" />`,
});
