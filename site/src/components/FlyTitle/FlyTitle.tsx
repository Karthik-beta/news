// The fly title (design/components.md, "Fly title"): the crimson topic line above every headline, Fira 600 15px,
// sentence case, two to four words naming the subject ("Apple's annual results"). 12px above the headline; the
// page template sets the gap. A page-template part: the text is the page's own head copy.
import { RefText } from "../SourceRef/RefText";

export interface FlyTitleProps {
  /** The page's fly title; {ref} tokens are filled in from data (rare, but allowed). */
  text: string;
  /** In lists, the fly title links to its page with the headline. */
  href?: string;
  /**
   * The text is final, not head copy: a page record's fly title in a list (home, hubs). Written exactly as stored,
   * with no {ref} tokens filled in, so it needs no page data.
   */
  plain?: boolean;
}

export function FlyTitle({ text, href, plain = false }: FlyTitleProps) {
  const words = plain ? text : <RefText text={text} cite="plain" />;
  return (
    <p className="fly-title text-fly">
      {href ? (
        <a className="fly-title__link" href={href}>
          {words}
        </a>
      ) : (
        words
      )}
    </p>
  );
}
