// The glossary fixture pages use until the site glossary exists. Entries link to the list of them in the
// GlossaryTerm gallery, so every glossary link in a fixture build resolves (the internal link check).
import type { Glossary } from "../../components/GlossaryTerm/GlossaryTerm";

export const GLOSSARY_FIXTURE_PAGE = "/fixtures/primitives/GlossaryTerm/";
export const glossaryEntryId = (slug: string) => `glossary-${slug}`;

const entries: Record<string, { term: string; definition: string }> = {
  "core-inflation": {
    term: "Core inflation",
    definition: "The change in consumer prices leaving out food and energy, whose prices swing the most.",
  },
  "gross-margin": {
    term: "Gross margin",
    definition: "Sales minus the cost of the goods and services sold, as a share of sales.",
  },
  "basis-point": {
    term: "Basis point",
    definition: "One hundredth of a percentage point. A cut of 25 basis points takes a rate from 5.25% to 5.00%.",
  },
  "randomised-controlled-trial": {
    term: "Randomised controlled trial",
    definition:
      "A study that assigns people at random to a treatment or a comparison group, so differences between the groups can be put down to the treatment.",
  },
};

export const fixtureGlossary: Glossary = Object.fromEntries(
  Object.entries(entries).map(([slug, e]) => [
    slug,
    { ...e, href: `${GLOSSARY_FIXTURE_PAGE}#${glossaryEntryId(slug)}` },
  ]),
);
