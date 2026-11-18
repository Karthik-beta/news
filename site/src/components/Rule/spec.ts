import { definePrimitive } from "../contract";

export default definePrimitive({
  name: "Rule",
  purpose: "The four rules of design/layout.md (hairline, ink, section, brand with its tag), as an element or classes.",
  example: `<Rule kind="section" />
<figure className={ruleTop("brand")}>…</figure>`,
});
