// Pagination (design/components.md, "Pagination"): "Newer" and "Older" text links at the ends and page numbers
// between, Fira 15. The current page in ink, weight 600, with a 2px crimson underline (current navigation); the
// others in ink-2. Page 1 is the newest. Long runs keep the first and last pages and the current page's
// neighbours, with an ellipsis for each gap. Plain links, so it works with no JavaScript. A page-template part
// (hubs, search, latest). Static.
import { VisuallyHidden } from "../VisuallyHidden/VisuallyHidden";

export interface PaginationProps {
  /** The page shown, from 1 (the newest). */
  current: number;
  /** How many pages there are. With one or none, nothing is shown. */
  total: number;
  /** The list's first page ("/finance/"); later pages add `?page=n`. */
  base?: string;
  /** A page's address, when the list's pages are not `?page=n`. */
  href?: (page: number) => string;
  /** The navigation landmark's name. */
  label?: string;
}

/** "/finance/" for page 1, "/finance/?page=3" for page 3 (or "&page=3" when the base has a query). */
export const pageHref = (base: string, page: number) =>
  page <= 1 ? base : `${base}${base.includes("?") ? "&" : "?"}page=${page}`;

/** The pages to show: the first, the last, the current page and its neighbours, "gap" for each run left out. */
export function pageWindow(current: number, total: number): (number | "gap")[] {
  const keep = new Set([1, total, current - 1, current, current + 1].filter((n) => n >= 1 && n <= total));
  const pages = [...keep].sort((a, b) => a - b);
  const out: (number | "gap")[] = [];
  for (const n of pages) {
    const prev = out.at(-1);
    // A run of exactly one page shows that page; an ellipsis would take as much room and say less.
    if (typeof prev === "number" && n - prev === 2) out.push(prev + 1);
    else if (typeof prev === "number" && n - prev > 2) out.push("gap");
    out.push(n);
  }
  return out;
}

export function Pagination({ current, total, base = "/", href, label = "Pages" }: PaginationProps) {
  // A count or page number that is not a finite number shows nothing, or page 1, never "Page NaN".
  const pages = Number.isFinite(total) ? Math.max(0, Math.floor(total)) : 0;
  if (pages <= 1) return null;
  const at = Math.min(Math.max(1, Number.isFinite(current) ? Math.floor(current) : 1), pages);
  const link = (n: number) => (href ? href(n) : pageHref(base, n));
  return (
    <nav className="pagination" aria-label={label}>
      <ul className="pagination__list">
        {at > 1 ? (
          <li>
            <a className="pagination__end" href={link(at - 1)} rel="prev">
              Newer
            </a>
          </li>
        ) : null}
        {pageWindow(at, pages).map((n, i) =>
          n === "gap" ? (
            // biome-ignore lint/suspicious/noArrayIndexKey: at most two gaps, fixed by position
            <li key={`gap-${i}`} className="pagination__gap" aria-hidden="true">
              …
            </li>
          ) : (
            <li key={n}>
              <a className="pagination__page" href={link(n)} aria-current={n === at ? "page" : undefined}>
                <VisuallyHidden>Page </VisuallyHidden>
                {n}
              </a>
            </li>
          ),
        )}
        {at < pages ? (
          <li>
            <a className="pagination__end" href={link(at + 1)} rel="next">
              Older
            </a>
          </li>
        ) : null}
      </ul>
    </nav>
  );
}
