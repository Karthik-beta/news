import { fixtureGlossary, glossaryEntryId } from "../../../renderer/fixtures/glossary";
import type { FixtureCase } from "../../contract";
import { GlossaryProvider, GlossaryTerm } from "../GlossaryTerm";

export const cases: FixtureCase[] = [
  {
    name: "in a sentence",
    render: () => (
      <GlossaryProvider glossary={fixtureGlossary}>
        <p className="text-body">
          Prices excluding food and energy, or <GlossaryTerm slug="core-inflation">core inflation</GlossaryTerm>, rose
          more slowly than in the month before.
        </p>
      </GlossaryProvider>
    ),
  },
  {
    name: "long definition, at the end of a line",
    render: () => (
      <GlossaryProvider glossary={fixtureGlossary}>
        <p className="text-body">
          The study was a <GlossaryTerm slug="randomised-controlled-trial">randomised controlled trial</GlossaryTerm>
        </p>
      </GlossaryProvider>
    ),
  },
  {
    name: "a term the glossary does not define reads as plain text",
    render: () => (
      <GlossaryProvider glossary={fixtureGlossary}>
        <p className="text-body">
          The bank cited <GlossaryTerm slug="not-in-the-glossary">neutral rate estimates</GlossaryTerm> in its
          statement.
        </p>
      </GlossaryProvider>
    ),
  },
  {
    name: "no glossary supplied: plain text",
    render: () => (
      <p className="text-body">
        <GlossaryTerm slug="core-inflation">core inflation</GlossaryTerm> with no glossary provider.
      </p>
    ),
  },
  {
    name: "the fixture glossary (the entries the terms link to)",
    render: () => (
      <dl className="text-body-s">
        {Object.entries(fixtureGlossary).map(([slug, e]) => (
          <div key={slug} id={glossaryEntryId(slug)}>
            <dt>
              <strong>{e.term}</strong>
            </dt>
            <dd>{e.definition}</dd>
          </div>
        ))}
      </dl>
    ),
  },
];
