import { definePrimitive } from "../contract";

export default definePrimitive({
  name: "StateMessage",
  purpose:
    "Page-template part: the empty or error state, one plain sentence in the reading face plus one link onwards.",
  example: `<StateMessage message="No pages match these filters." action={{ href: "/search/?q=apple", label: "Clear the region filter" }} />`,
});
