import { definePrimitive } from "../contract";

export default definePrimitive({
  name: "Headline",
  purpose: "Page-template part: the headline from the page's head copy, xl on articles, display/m/s in lists.",
  example: `<Headline text={head.headline} />  <Headline text={page.headline} size="s" level={3} href={page.url} />`,
});
