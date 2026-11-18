// A cited figure: resolves a data reference (dp_… or dv_…), writes the value through the shared formatters, and
// opens the citation popover (design/components.md, "Citation popover") with the exact value as reported, the
// document, where in it, the retrieval date and a link to the source. Zero JavaScript: the popover is native.
// Missing data renders as "No data", never as a zero.
import { useId } from "react";
import { formatDate } from "../../format";
import type { SourceT } from "../../renderer/bundle/schema";
import { usePageData } from "../../renderer/data/context";
import {
  type CitedSource,
  formatValue,
  MISSING_TEXT,
  type ResolvedValue,
  type ValueStyle,
} from "../../renderer/data/resolve";
import { Icon } from "../Icon/Icon";
import { Popover } from "../Popover/Popover";

type Resolved = Extract<ResolvedValue, { ok: true }>;

export interface SourceRefProps {
  /** dp_… or dv_… */
  dataRef: string;
  /** Compact ("$391.0bn", the default) or the source's own digits ("$391,035 million"). */
  style?: ValueStyle;
  /** True when a nearby subtitle names the currency (allows ¥ for yen and yuan). */
  currencyNamed?: boolean;
  className?: string;
}

/** "XBRL tag us-gaap:Revenues", "Page 17, table 2": where in the document the value is. */
export function describeLocator(locator: Resolved["locator"]): string | null {
  if (!locator) return null;
  const l = locator as Record<string, unknown>;
  const str = (k: string) => (typeof l[k] === "string" || typeof l[k] === "number" ? String(l[k]) : null);
  switch (l.kind) {
    case "xbrl":
      return str("tag") ? `XBRL tag ${str("tag")}` : null;
    case "pdf": {
      const parts = [str("page") && `Page ${str("page")}`, str("table") && `table ${str("table")}`].filter(Boolean);
      return parts.length ? parts.join(", ") : null;
    }
    case "json":
      return str("series") ? `Series ${str("series")}` : null;
    case "sdmx":
      return str("key") ? `Series ${str("key")}` : null;
    default:
      return null;
  }
}

const retrieved = (s: SourceT) => `Retrieved ${formatDate(s.retrieved_at)}`;

function SourceLines({ sources }: { sources: CitedSource[] }) {
  return (
    <>
      {sources.map(({ id, source }) => (
        <span key={id} className="source-ref__source">
          <span className="popover__line">{source.title}</span>
          <span className="popover__meta">{retrieved(source)}</span>
          <a className="popover__link" href={source.url}>
            Open the source <Icon name="external" />
          </a>
        </span>
      ))}
    </>
  );
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
  const shown = formatValue(value, style, { currencyNamed });
  const exact = formatValue(value, "as-reported", { currencyNamed });
  const where = describeLocator(value.locator);
  return (
    <Popover
      id={id}
      label={`Source for ${shown}`}
      trigger={shown}
      triggerClassName={`source-ref${className ? ` ${className}` : ""}`}
    >
      <span className="popover__value">{exact}</span>
      <span className="popover__line">
        {value.label}
        {value.period ? `, ${value.period.label}` : ""}
      </span>
      {value.derived ? <span className="popover__meta">Calculated by News from the figures below.</span> : null}
      {where ? <span className="popover__meta">{where}</span> : null}
      <SourceLines sources={value.sources} />
    </Popover>
  );
}
