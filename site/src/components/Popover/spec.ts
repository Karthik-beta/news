import { definePrimitive } from "../contract";

export default definePrimitive({
  name: "Popover",
  purpose: "Shared popover look: <Popover> (native popover, 0 KB JS) and <Tooltip> for islands that position it.",
  example: `<Popover id="src-1" label="Source for $391.0bn" trigger="$391.0bn">
  <span className="popover__value">$391,035 million</span>
</Popover>`,
});
