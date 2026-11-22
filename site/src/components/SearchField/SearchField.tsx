// The search field (design/components.md, "Search field"): full width, 48px tall, Fira 18, the magnifier inside on
// the left and a "Search" button on the right; filters as one row of selects below. A plain GET form, so search
// works with no JavaScript: the query is `q` and each filter select sends its own name.
import type { ReactNode } from "react";
import { Button } from "../Button/Button";
import { Icon } from "../Icon/Icon";

export interface SearchFieldProps {
  /** The search page. */
  action?: string;
  /** The query the page was rendered for. */
  query?: string;
  /** The input's id; unique on the page. */
  id?: string;
  /** The search landmark's and the input's name. Change it only where two fields share a page. */
  label?: string;
  /** Selects (or other fields) sent with the query, in one row below. */
  filters?: ReactNode;
}

export function SearchField({
  action = "/search/",
  query,
  id = "search-q",
  label = "Search pages",
  filters,
}: SearchFieldProps) {
  return (
    // biome-ignore lint/a11y/useSemanticElements: the role sits on the form, so the landmark and the form are one element
    <form className="search-field" role="search" aria-label={label} action={action} method="get">
      <div className="search-field__row">
        <label className="visually-hidden" htmlFor={id}>
          {label}
        </label>
        <span className="search-field__box">
          <Icon name="search" className="search-field__icon" />
          <input className="search-field__input" id={id} type="search" name="q" defaultValue={query} />
        </span>
        <Button variant="primary" type="submit" className="search-field__submit">
          Search
        </Button>
      </div>
      {filters ? <div className="search-field__filters">{filters}</div> : null}
    </form>
  );
}
