import { definePrimitive } from "../contract";

export default definePrimitive({
  name: "LoadingRegion",
  purpose: "Loading state for dynamic lists: previous results at 50% opacity while busy, a polite status, no spinner.",
  example: `<LoadingRegion busy={loading}><StoryList … /></LoadingRegion>`,
});
