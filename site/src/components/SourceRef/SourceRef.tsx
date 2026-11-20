// A cited figure: resolves a data reference (dp_… or dv_…), writes the value through the shared formatters, and
// opens the citation popover (design/components.md, "Citation popover"; the SourcePopover primitive) with the
// exact value as reported, the document, where in it, the retrieval date and a link to the source. Zero
// JavaScript: the popover is native. Missing data renders as "No data", never as a zero.
import { useId } from "react";
import { usePageData } from "../../renderer/data/context";
import { formatValue, MISSING_TEXT, type ValueStyle } from "../../renderer/data/resolve";
import { SourcePopover } from "../SourcePopover/SourcePopover";

export { describeLocator } from "../SourcePopover/SourcePopover";

export interface SourceRefProps {
  /** dp_… or dv_… */
  dataRef: string;
  /** Compact ("$391.0bn", the default) or the source's own digits ("$391,035 million"). */
  style?: ValueStyle;
  /** True when a nearby subtitle names the currency (allows ¥ for yen and yuan). */
  currencyNamed?: boolean;
  className?: string;
}

export function SourceRef({ dataRef, style = "compact", currencyNamed, className }: SourceRefProps) {
  const data = usePageData();
  const id = `cite-${useId().replace(/[^a-zA-Z0-9_-]/g, "")}`;
  const value = data.value(dataRef);
  if (!value.ok) {
    return (
      <span className={`source-ref source-ref--missing${className ? ` ${className}` : ""}`} data-ref={dataRef}>
        {MISSING_TEXT}
      </span>
    );
  }
  return (
    <SourcePopover
      id={id}
      value={value}
      shown={formatValue(value, style, { currencyNamed })}
      triggerClassName={`source-ref${className ? ` ${className}` : ""}`}
      currencyNamed={currencyNamed}
    />
  );
}
