import { definePrimitive } from "../contract";

export default definePrimitive({
  name: "GlossaryTerm",
  purpose:
    "A glossary term inside text: dotted underline, native popover with the definition and a link to the entry (0 KB JS).",
  example: `<GlossaryProvider glossary={siteGlossary}>
  … prices excluding food and energy, or <GlossaryTerm slug="core-inflation">core inflation</GlossaryTerm>, rose …
</GlossaryProvider>
// In page copy (Prose, Note, MethodSummary) the writer marks a term as [core inflation](glossary:core-inflation).`,
});
