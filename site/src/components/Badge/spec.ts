import { definePrimitive } from "../contract";

export default definePrimitive({
  name: "Badge",
  purpose:
    "Page-template part: a source badge (Primary, Context, Preprint, Peer-reviewed, Dataset, Press release, Filing).",
  example: `<Badge kind={roleBadge(source)} />  {typeBadge(source) && <Badge kind={typeBadge(source)} />}`,
});
