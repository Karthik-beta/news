// Text with `{ref}` tokens, rendered: cited values become SourceRefs (or plain cited spans), periods and context
// lines become their labels, quotes get quotation marks, and missing references say "no data". The model writes
// no digits; every number in the text comes from data.json through the shared formatters.
import { usePageData } from "../../renderer/data/context";
import { formatValue, MISSING_TEXT } from "../../renderer/data/resolve";
import { SourceRef } from "./SourceRef";

export interface RefTextProps {
  text: string;
  /**
   * "popover": values open the citation popover (prose, standfirsts). "plain": values are cited by a data-ref
   * attribute but not interactive (headlines, titles).
   */
  cite?: "popover" | "plain";
}

export function RefText({ text, cite = "popover" }: RefTextProps) {
  const data = usePageData();
  return (
    <>
      {data.text(text).map((segment, i) => {
        const key = `${i}-${segment.type}`;
        switch (segment.type) {
          case "text":
            return segment.text;
          case "value":
            return cite === "popover" ? (
              <SourceRef key={key} dataRef={segment.ref} />
            ) : (
              <span key={key} className="cited" data-ref={segment.ref}>
                {formatValue(segment.value)}
              </span>
            );
          case "period":
          case "context":
            return segment.type === "period" ? segment.label : segment.text;
          case "quote":
            return `“${segment.text}”`;
          case "missing":
            return (
              <span key={key} className="source-ref--missing" data-ref={segment.ref}>
                {MISSING_TEXT.toLowerCase()}
              </span>
            );
          default:
            return null;
        }
      })}
    </>
  );
}
