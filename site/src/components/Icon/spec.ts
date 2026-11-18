import { definePrimitive } from "../contract";

export default definePrimitive({
  name: "Icon",
  purpose: "The eight icons (search, external, download, close, play, error, check, info), 1em, in currentColor.",
  example: `<a href={url}>Open the source <Icon name="external" /></a>
<button type="button"><Icon name="close" label="Close" /></button>`,
});
