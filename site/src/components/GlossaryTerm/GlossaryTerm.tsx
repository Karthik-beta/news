// A term defined in the site glossary (design/components.md, "Glossary term"): a dotted 1px tertiary-ink underline;
// tapping, clicking, Enter or Space opens the definition in the shared popover with a link to the glossary entry.
// Zero JavaScript: it is the #4 native popover, so Escape and a tap outside close it and focus returns to the term.
//
// Definitions are not page copy. They come from the site glossary, which the page template hands in through
// <GlossaryProvider>. A term the glossary does not define renders as plain text: never a made-up definition.
import { createContext, type ReactNode, useContext, useId } from "react";
import { Popover } from "../Popover/Popover";

export interface GlossaryEntry {
  /** The term as the glossary heads it ("Core inflation"). */
  term: string;
  /** One or two plain sentences. */
  definition: string;
  /** The entry's page; defaults to /glossary/#<slug>. */
  href?: string;
}

/** Glossary entries by slug ("core-inflation"). */
export type Glossary = Readonly<Record<string, GlossaryEntry>>;

/** Slugs are lower-case words joined by hyphens: "core-inflation", "basis-point". */
export const GLOSSARY_SLUG = /^[a-z0-9]+(-[a-z0-9]+)*$/;

const GlossaryContext = createContext<Glossary>({});

/** Supplies the site glossary to every GlossaryTerm below it. Without one, terms render as plain text. */
export function GlossaryProvider({ glossary, children }: { glossary: Glossary; children: ReactNode }) {
  return <GlossaryContext.Provider value={glossary}>{children}</GlossaryContext.Provider>;
}

export const useGlossary = (): Glossary => useContext(GlossaryContext);

export const glossaryHref = (slug: string, entry?: GlossaryEntry) => entry?.href ?? `/glossary/#${slug}`;

export interface GlossaryTermProps {
  /** The words as they read in the sentence ("core prices"). */
  children: string;
  /** The glossary entry's slug. */
  slug: string;
}

export function GlossaryTerm({ children, slug }: GlossaryTermProps) {
  const entry = useGlossary()[slug];
  const id = `gloss-${useId().replace(/[^a-zA-Z0-9_-]/g, "")}`;
  if (!entry) return <>{children}</>;
  return (
    <Popover id={id} trigger={children} triggerClassName="glossary-term" label={`Definition of ${entry.term}`}>
      <span className="popover__value">{entry.term}</span>
      <span className="popover__line glossary-term__definition">{entry.definition}</span>
      <a className="popover__link" href={glossaryHref(slug, entry)}>
        Read the glossary entry
      </a>
    </Popover>
  );
}
