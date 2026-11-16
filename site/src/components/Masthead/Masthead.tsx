// The masthead (design/components.md, "Masthead"): the wordmark, the section links and Search, on one row with a
// hairline underneath. The current section has a 2px crimson underline. On phones the links wrap to a second row
// under the wordmark. Not sticky. A page-template part; the wordmark is placeholder text until the final name.
import { Icon } from "../Icon/Icon";

export interface NavLink {
  label: string;
  href: string;
}

/** The five categories (News — Product Requirements Document). */
export const SECTIONS: readonly NavLink[] = [
  { label: "Tech", href: "/tech/" },
  { label: "Business", href: "/business/" },
  { label: "Finance", href: "/finance/" },
  { label: "Economy", href: "/economy/" },
  { label: "Science", href: "/science/" },
];

export const WORDMARK = "News";

export interface MastheadProps {
  sections?: readonly NavLink[];
  /** The href of the section the page belongs to, if any. */
  current?: string | null;
  homeHref?: string;
  searchHref?: string;
  /** The navigation landmark's name. Change it only where two mastheads share a page (fixture galleries). */
  navLabel?: string;
}

export function Masthead({
  sections = SECTIONS,
  current = null,
  homeHref = "/",
  searchHref = "/search/",
  navLabel = "Sections",
}: MastheadProps) {
  return (
    <header className="masthead">
      <div className="masthead__inner">
        <a className="masthead__wordmark text-wordmark" href={homeHref}>
          {WORDMARK}
        </a>
        <nav className="masthead__nav" aria-label={navLabel}>
          <ul className="masthead__links">
            {sections.map((s) => (
              <li key={s.href}>
                <a className="masthead__link" href={s.href} aria-current={s.href === current ? "page" : undefined}>
                  {s.label}
                </a>
              </li>
            ))}
          </ul>
        </nav>
        <a className="masthead__search" href={searchHref}>
          <Icon name="search" />
          Search
        </a>
      </div>
    </header>
  );
}
