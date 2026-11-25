// A story item (design/components.md, "Story item"), used in home rows, hubs and search results: fly title,
// headline (headline-s), an optional key figure (figure-s) with its sparkline, a one-sentence summary (body-s,
// ink-2), and the page type and region (Fira 13.5, ink-3). Hub and search items have a date column on the left
// (above the item on phones) and sit in a StoryList with hairlines between them. Every word comes from the page
// record: the item takes a page ID (and on the home page a figure or chart ID), never text. A page that is not in
// the listings is left out, so a list never shows an empty slot. Static: 0 KB of JavaScript.
import { formatShortDate } from "../../format";
import { PAGE_TYPE_LABELS, type PageListing, regionLabel, useListings } from "../../renderer/data/listings";
import { FlyTitle } from "../FlyTitle/FlyTitle";
import { Headline } from "../Headline/Headline";
import { SlotFigure } from "./SlotFigure";

export type StoryVariant = "home" | "hub" | "search";

export interface StoryItemProps {
  pageId: string;
  /** Home and hub: one of the page's key figures (dp_ or dv_). */
  figure?: string | null;
  /** Home: one of the page's charts, by chart ID, as a mini chart. */
  chart?: string | null;
  /** "home" (no date column), "hub" or "search" (date column; search results show no figure). */
  variant?: StoryVariant;
  /** "s" in rows and lists; "m" for the home page's two side stories. */
  size?: "s" | "m";
  /** The headline's level under the list's heading. */
  level?: 2 | 3 | 4;
}

/** "Earnings, North America": the page type and region, comma-separated (no middle dots: content-style.md). */
export const listingMeta = (r: Pick<PageListing, "page_type" | "region">): string =>
  `${PAGE_TYPE_LABELS[r.page_type] ?? r.page_type}, ${regionLabel(r.region)}`;

export function StoryItem({ pageId, figure, chart, variant = "home", size = "s", level = 3 }: StoryItemProps) {
  const page = useListings().get(pageId);
  if (!page) return null;
  const r = page.record;
  const dated = variant !== "home";
  return (
    <article className={`story-item story-item--${variant}`}>
      {dated ? (
        <p className="story-item__date">
          <time dateTime={r.published_at}>{formatShortDate(r.published_at)}</time>
        </p>
      ) : null}
      <div className="story-item__body">
        {r.fly_title ? <FlyTitle text={r.fly_title} plain /> : null}
        <Headline text={r.title} size={size} level={level} href={r.path} plain />
        {variant !== "search" ? <SlotFigure page={page} figure={figure} chart={chart} /> : null}
        <p className="story-item__summary text-body-s">{r.summary}</p>
        <p className="story-item__meta">{listingMeta(r)}</p>
      </div>
    </article>
  );
}

export interface StoryListProps {
  /** Page IDs in the list's order (hubs: newest first). */
  pageIds: readonly string[];
  variant?: Exclude<StoryVariant, "home">;
  level?: 2 | 3 | 4;
}

/** A hub or search list: story items with a date column, hairlines between them. Missing pages are left out. */
export function StoryList({ pageIds, variant = "hub", level = 2 }: StoryListProps) {
  const listings = useListings();
  // A page is listed once, even if the list names it twice.
  const ids = [...new Set(pageIds)].filter((id) => listings.get(id) !== null);
  if (ids.length === 0) return null;
  return (
    <ol className={`story-list story-list--${variant}`}>
      {ids.map((id) => (
        <li key={id} className="story-list__item">
          <StoryItem pageId={id} variant={variant} level={level} />
        </li>
      ))}
    </ol>
  );
}
