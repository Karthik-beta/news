// The fly title (design/components.md, "Fly title"): the crimson topic line above every headline, Fira 600 15px,
// sentence case, two to four words naming the subject ("Apple's annual results"). 12px above the headline; the
// page template sets the gap. A page-template part: the text is the page's own head copy.
import { RefText } from "../SourceRef/RefText";

export interface FlyTitleProps {
  /** The page's fly title; {ref} tokens are filled in from data (rare, but allowed). */
  text: string;
  /** In lists, the fly title links to its page with the headline. */
  href?: string;
}

export function FlyTitle({ text, href }: FlyTitleProps) {
  const words = <RefText text={text} cite="plain" />;
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
