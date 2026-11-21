// The Latest list (design/components.md, "Latest list"): home only, under the lead block. The automatic list of
// the newest pages (not curated): a section rule, the label "Latest", then one line per page: the time if it was
// published today, else the short date (Fira, ink-3, tabular), then the fly title (Fira 600, crimson) and the
// headline (Literata) on one line, wrapping on phones; hairlines between items. Ends with "All latest pages".
// Times are UTC, like every date the shared formatters write. A page-template part. Static.
import { useId } from "react";
import { formatShortDate, formatTimeOfDay, parseIsoDate } from "../../format";
import { type ListedPage, useListings } from "../../renderer/data/listings";

/** The list shows at most this many pages (the layout's 8 to 10). */
export const LATEST_MAX = 10;

export interface LatestListProps {
  /** The newest pages, newest first (HUB#latest). */
  pageIds: readonly string[];
  /** The time the home page is rendered (RFC 3339, UTC): pages published on its date show their time. */
  now: string;
  /** The full Latest page. */
  allHref?: string;
}

const sameDay = (a: string, b: string) => {
  const x = parseIsoDate(a);
  const y = parseIsoDate(b);
  return x.year === y.year && x.month === y.month && x.day === y.day;
};

/** "10:42" for a page published on `now`'s date, "1 Nov 2024" otherwise. */
export const latestWhen = (publishedAt: string, now: string) =>
  sameDay(publishedAt, now) ? formatTimeOfDay(publishedAt) : formatShortDate(publishedAt);

export function LatestList({ pageIds, now, allHref = "/latest/" }: LatestListProps) {
  const headingId = `latest-${useId().replace(/[^a-zA-Z0-9_-]/g, "")}`;
  const listings = useListings();
  // A page is listed once, even if the list names it twice.
  const pages = [...new Set(pageIds)]
    .map((id) => listings.get(id))
    .filter((p): p is ListedPage => p !== null)
    .slice(0, LATEST_MAX);
  if (pages.length === 0) return null;
  return (
    <section className="latest-list" aria-labelledby={headingId}>
      <h2 className="latest-list__label" id={headingId}>
        Latest
      </h2>
      <ol className="latest-list__items">
        {pages.map(({ record: r }) => (
          <li key={r.page_id} className="latest-list__item">
            <time className="latest-list__when" dateTime={r.published_at}>
              {latestWhen(r.published_at, now)}
            </time>
            <a className="latest-list__link" href={r.path}>
              {r.fly_title ? (
                <>
                  <span className="latest-list__fly">{r.fly_title}</span>{" "}
                </>
              ) : null}
              <span className="latest-list__headline">{r.title}</span>
            </a>
          </li>
        ))}
      </ol>
      <p className="latest-list__all">
        <a href={allHref}>All latest pages</a>
      </p>
    </section>
  );
}
