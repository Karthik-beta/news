// Static-page copy to HTML. Glossary links ([words](glossary:slug)) become the GlossaryTerm component, rendered
// with the site glossary (the entries of content/pages/glossary.md); a slug the glossary lacks stays plain words.
import { renderToString } from "react-dom/server";
import { GlossaryProvider, GlossaryTerm } from "../components/GlossaryTerm/GlossaryTerm";
import { type GlossaryItem, glossaryItems } from "./content";
import { type Block, plainText, renderBlocks } from "./markdown";

export const siteGlossary = (items: readonly GlossaryItem[] = glossaryItems()) =>
  Object.fromEntries(items.map((g) => [g.slug, { term: g.term, definition: plainText(g.definition) }]));

/** The blocks as HTML for a page body. Pass one `prefix` per page, so popover ids never repeat across pages' parts. */
export function renderCopy(blocks: readonly Block[], glossary = siteGlossary(), prefix = "c"): string {
  let n = 0;
  return renderBlocks(blocks, {
    term: (words, slug) =>
      renderToString(
        <GlossaryProvider glossary={glossary}>
          <GlossaryTerm slug={slug}>{words}</GlossaryTerm>
        </GlossaryProvider>,
        { identifierPrefix: `${prefix}${n++}` },
      ),
  });
}
