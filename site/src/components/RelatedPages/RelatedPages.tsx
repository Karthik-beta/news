// Related pages (design/components.md, "Related list"): a rail block, "More on Apple": up to three pages, each its
// headline with a small meta line under it ("Earnings, 1 August 2024"), then a link to the entity's hub. The
// entity's name comes from the page record's entities; every headline and date from the related page's own
// record. Pages not in the listings are left out; with none left, the block is not shown. A page-template part.
import { useId } from "react";
import { formatDate } from "../../format";
import { type ListedPage, PAGE_TYPE_LABELS, useListings } from "../../renderer/data/listings";
import { Headline } from "../Headline/Headline";

/** The block shows at most three pages. */
export const RELATED_MAX = 3;

export interface RelatedPagesProps {
  /** The entity, as the page record names it ("Apple"), and its hub. */
  entity: { name: string; href: string };
  /** Related page IDs, most relevant first. */
  pageIds: readonly string[];
}

export function RelatedPages({ entity, pageIds }: RelatedPagesProps) {
  const headingId = `related-${useId().replace(/[^a-zA-Z0-9_-]/g, "")}`;
  const listings = useListings();
  const pages = pageIds
    .map((id) => listings.get(id))
    .filter((p): p is ListedPage => p !== null)
    .slice(0, RELATED_MAX);
  if (pages.length === 0) return null;
  return (
    <section className="related-pages" aria-labelledby={headingId}>
      <h2 className="related-pages__heading" id={headingId}>
        More on {entity.name}
      </h2>
      <ul className="related-pages__list">
        {pages.map(({ record: r }) => (
          <li key={r.page_id} className="related-pages__item">
            <Headline text={r.title} size="s" level={3} href={r.path} plain />
            <p className="related-pages__meta">
              {PAGE_TYPE_LABELS[r.page_type] ?? r.page_type},{" "}
              <time dateTime={r.published_at}>{formatDate(r.published_at)}</time>
            </p>
          </li>
        ))}
      </ul>
      <p className="related-pages__all">
        <a href={entity.href}>All pages on {entity.name}</a>
      </p>
    </section>
  );
}
