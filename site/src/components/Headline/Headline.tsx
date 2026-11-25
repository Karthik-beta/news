// The headline (design/components.md, "Headline"): headline-xl on articles; display, headline-m or headline-s in
// lists. Balanced lines. A linked headline shows its underline only on hover. Values in a headline are cited by
// data-ref but open no popover (the standfirst and key figures carry the citations). A page-template part.
import { RefText } from "../SourceRef/RefText";

export type HeadlineSize = "xl" | "display" | "m" | "s";

export interface HeadlineProps {
  text: string;
  /** "xl" on article pages (the default); "display" for the home lead; "m" and "s" in lists. */
  size?: HeadlineSize;
  /** The heading level: 1 on the article page; lists choose theirs. */
  level?: 1 | 2 | 3 | 4;
  href?: string;
  /** The text is final: a page record's title in a list. Written exactly as stored; needs no page data. */
  plain?: boolean;
}

const CLASS: Record<HeadlineSize, string> = {
  xl: "text-headline-xl",
  display: "text-display",
  m: "text-headline-m",
  s: "text-headline-s",
};

export function Headline({ text, size = "xl", level = 1, href, plain = false }: HeadlineProps) {
  const Tag = `h${level}` as "h1";
  const words = plain ? text : <RefText text={text} cite="plain" />;
  return (
    <Tag className={`headline headline--${size} ${CLASS[size]}`}>
      {href ? (
        <a className="headline__link" href={href}>
          {words}
        </a>
      ) : (
        words
      )}
    </Tag>
  );
}
