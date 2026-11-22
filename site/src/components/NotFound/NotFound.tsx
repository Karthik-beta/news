// The 404 page body (design/components.md, "404"): the site's only centred layout. "This page doesn't exist." in
// Literata 700, a line on why, the search field and links to the five categories.
import { type NavLink, SECTIONS } from "../Masthead/Masthead";
import { SearchField } from "../SearchField/SearchField";

export interface NotFoundProps {
  sections?: readonly NavLink[];
  searchAction?: string;
  /** Names for the search and category landmarks. Change them only where two share a page (fixture galleries). */
  searchLabel?: string;
  searchId?: string;
  navLabel?: string;
}

export function NotFound({
  sections = SECTIONS,
  searchAction,
  searchLabel = "Search pages",
  searchId = "not-found-q",
  navLabel = "Categories",
}: NotFoundProps) {
  return (
    <div className="not-found">
      <h1 className="not-found__title">This page doesn't exist.</h1>
      <p className="not-found__text">It may have moved, or the link may be wrong.</p>
      <SearchField action={searchAction} id={searchId} label={searchLabel} />
      <nav aria-label={navLabel}>
        <ul className="not-found__links">
          {sections.map((s) => (
            <li key={s.href}>
              <a className="not-found__link" href={s.href}>
                {s.label}
              </a>
            </li>
          ))}
        </ul>
      </nav>
    </div>
  );
}
