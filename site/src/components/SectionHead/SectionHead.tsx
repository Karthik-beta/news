// A section head (design/components.md, "Section head"): a 2px ink rule; the section's name (headline-l) on the
// left; a link to its hub on the right ("All economy pages", Fira, ink-2, underlined), under the name on narrow
// screens. The name is the taxonomy's category name, never curator text; the link words are made from it.
// A page-template part (home sections). Static.

export interface SectionHeadProps {
  /** The category's name from the taxonomy ("Economy"). */
  name: string;
  /** The category hub. Without it, no link. */
  href?: string;
  level?: 2 | 3;
  /** The heading's id, for the section's aria-labelledby. */
  id?: string;
}

/** "All economy pages": the hub link's words, from the category name. */
export const allPagesText = (name: string) => `All ${name.toLowerCase()} pages`;

export function SectionHead({ name, href, level = 2, id }: SectionHeadProps) {
  const Tag = `h${level}` as "h2";
  return (
    <div className="section-head">
      <Tag className="section-head__name text-headline-l" id={id}>
        {name}
      </Tag>
      {href ? (
        <a className="section-head__all" href={href}>
          {allPagesText(name)}
        </a>
      ) : null}
    </div>
  );
}
