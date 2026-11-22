// The standfirst (design/components.md, "Standfirst"): one or two sentences under the headline in type.standfirst,
// full ink, at most 58 characters a line. Its figures are cited: each opens the citation popover. A page-template
// part: the text is the page's own head copy.
import { RefText } from "../SourceRef/RefText";

export function Standfirst({ text, plain = false }: { text: string; plain?: boolean }) {
  return (
    <p className="standfirst text-standfirst">
      {/* plain: final text with no {ref} tokens (a static page's description), so it needs no page data. */}
      {plain ? text : <RefText text={text} />}
    </p>
  );
}
