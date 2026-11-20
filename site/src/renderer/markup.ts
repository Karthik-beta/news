// Static markup for pages: react-dom/server's renderToStaticMarkup, plus one fix. React writes the popoverTarget
// and dateTime props as `popoverTarget="…"` and `dateTime="…"` in server HTML; browsers read attribute names
// case-insensitively, but the HTML checks (and tidy output) want lower case, so they are written `popovertarget`
// and `datetime`. Text can never contain the pattern, because React escapes quotes in text as &quot;.
import type { ReactNode } from "react";
import { renderToStaticMarkup } from "react-dom/server";

export function staticMarkup(node: ReactNode): string {
  return renderToStaticMarkup(node)
    .replaceAll(' popoverTarget="', ' popovertarget="')
    .replaceAll(' dateTime="', ' datetime="');
}
