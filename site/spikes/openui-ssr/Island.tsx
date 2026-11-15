import { createElement } from "react";
import { renderToString } from "react-dom/server";
import { type IslandName, islands } from "./islands";

// Server side of an island. The island's own subtree is rendered with renderToString in a separate React
// root, so its markup (text separators, useId values) is exactly what hydrateRoot expects on the client,
// independent of where the chart sits in the OpenUI tree. The surrounding page is static markup.
export function Island({ name, props }: { name: IslandName; props: object }) {
  const html = renderToString(createElement(islands[name], props as never));
  return (
    <div
      className="oui-island"
      data-island={name}
      data-props={JSON.stringify(props)}
      // biome-ignore lint/security/noDangerouslySetInnerHtml: React's own server output for the island
      dangerouslySetInnerHTML={{ __html: html }}
    />
  );
}
