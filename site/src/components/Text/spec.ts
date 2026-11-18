import { definePrimitive } from "../contract";

export default definePrimitive({
  name: "Text",
  purpose: "The type scale of design/typography.md as classes (text-body, text-fly …) and a <Text variant> wrapper.",
  example: `<Text variant="fly">Apple's annual results</Text>
<Text as="h1" variant="headline-xl">Services reached a quarter of Apple's sales</Text>`,
});
