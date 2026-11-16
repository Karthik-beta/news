// A quote from a primary source (design/components.md, "Quote"): Literata 20px, indented 20px behind a 2px ink
// rule, normal quotation marks, no decorative marks. The text is a q_ reference, so it is always an exact substring
// of the stored source text; the attribution under it (speaker, role, document, date) is built from data.json and
// sources.json and links to the source. Static: 0 KB JS.
import type { ComponentRenderer } from "@openuidev/react-lang";
import { formatDate } from "../../format";
import { usePageData } from "../../renderer/data/context";
import type { PageDataResolver } from "../../renderer/data/resolve";
import { MISSING_TEXT } from "../../renderer/data/resolve";

export interface QuoteProps {
  quote: string;
}

export interface QuoteParts {
  text: string;
  /** "Jerome Powell, Federal Reserve chair, FOMC press conference, 18 September 2024" */
  attribution: string;
  url: string | null;
}

/** The quote and its attribution from data, or null when the reference does not resolve. */
export function quoteParts(ref: string, data: PageDataResolver): QuoteParts | null {
  const quote = data.quote(ref);
  if (!quote) return null;
  const record = data.data.quotes[ref];
  const cited = quote.sources[0];
  const parts = [
    record?.speaker,
    record?.role,
    cited?.source.short,
    record?.said_on ? formatDate(record.said_on) : null,
  ].filter((p): p is string => typeof p === "string" && p.trim() !== "");
  return { text: quote.text, attribution: parts.join(", "), url: cited?.source.url ?? null };
}

export const Quote: ComponentRenderer<QuoteProps> = ({ props }) => {
  const data = usePageData();
  const parts = quoteParts(props.quote, data);
  if (!parts) {
    return (
      <p className="quote quote--missing text-label" data-ref={props.quote}>
        {MISSING_TEXT}
      </p>
    );
  }
  return (
    <figure className="quote" data-ref={props.quote}>
      <blockquote className="quote__text" cite={parts.url ?? undefined}>
        <p>“{parts.text}”</p>
      </blockquote>
      {parts.attribution ? (
        <figcaption className="quote__attribution text-label">
          {parts.url ? <a href={parts.url}>{parts.attribution}</a> : parts.attribution}
        </figcaption>
      ) : null}
    </figure>
  );
};
