// On this page (design/components.md, "On this page"): a rail block on article pages. A 2px ink rule, the heading
// "On this page" (Literata 700), then plain text links to each section and chart, no numbering. The template
// passes the page's own section headings and chart titles with their anchors; titles may carry {ref} tokens,
// which are filled in from the page's data like the headings themselves. A page-template part. Static.
import { useId } from "react";
import { RefText } from "../SourceRef/RefText";

export interface ContentsItem {
  /** The anchor: a section heading's id or a chart's statement name. */
  id: string;
  /** The heading or chart title as the page writes it, with {ref} tokens. */
  text: string;
}

export interface PageContentsProps {
  items: readonly ContentsItem[];
}

export function PageContents({ items }: PageContentsProps) {
  const headingId = `contents-${useId().replace(/[^a-zA-Z0-9_-]/g, "")}`;
  if (items.length === 0) return null;
  return (
    <nav className="page-contents" aria-labelledby={headingId}>
      <h2 className="page-contents__heading" id={headingId}>
        On this page
      </h2>
      <ul className="page-contents__list">
        {items.map((item) => (
          <li key={item.id} className="page-contents__item">
            <a className="page-contents__link" href={`#${item.id}`}>
              <RefText text={item.text} cite="plain" />
            </a>
          </li>
        ))}
      </ul>
    </nav>
  );
}
