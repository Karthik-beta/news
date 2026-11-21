// The sources behind a piece of text with {ref} tokens: each value's sources and each quote's source, in order of
// first use. Periods and context lines carry no source of their own. Shared by the page-type components, which
// write one source line under a block of copy (Timeline events, Finding, PolicyDecision's facts).
import type { CitedSource, PageDataResolver } from "../../renderer/data/resolve";

export function textSources(data: PageDataResolver, text: string): CitedSource[] {
  const out = new Map<string, CitedSource>();
  const add = (list: readonly CitedSource[]) => {
    for (const s of list) if (!out.has(s.id)) out.set(s.id, s);
  };
  for (const segment of data.text(text)) {
    if (segment.type === "value") add(segment.value.sources);
    if (segment.type === "quote") add(data.quote(segment.ref)?.sources ?? []);
  }
  return [...out.values()];
}
