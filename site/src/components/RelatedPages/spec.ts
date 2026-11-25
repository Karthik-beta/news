import { definePrimitive } from "../contract";

export default definePrimitive({
  name: "RelatedPages",
  purpose: 'Page-template part (article rail): "More on <entity>", up to three pages by ID, then the entity hub link.',
  example: `<RelatedPages entity={{ name: entity.name, href: entity.path }} pageIds={related} />`,
});
