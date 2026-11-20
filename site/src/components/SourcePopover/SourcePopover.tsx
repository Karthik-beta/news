// The citation popover (design/components.md, "Citation popover"): for one cited value, the exact value as
// reported ("$391,035 million"), what it is and its period, "Calculated by News" for derived values, where in the
// document it sits (page, table or XBRL tag), and for each source the document, the retrieval date and "Open the
// source". Paper, a 1px rule border, no shadow, 12px padding, at most 320px wide (the Popover look). It opens
// from its trigger on click, tap, Enter or Space and closes on Escape or a tap outside: the browser's own popover,
// so it ships no JavaScript. SourceRef renders every cited figure through it.
import { formatDate } from "../../format";
import type { SourceT } from "../../renderer/bundle/schema";
import { type CitedSource, formatValue, type ResolvedValue } from "../../renderer/data/resolve";
import { Icon } from "../Icon/Icon";
import { Popover } from "../Popover/Popover";

export type CitedValue = Extract<ResolvedValue, { ok: true }>;

/** "XBRL tag us-gaap:Revenues", "Page 17, table 2": where in the document the value is. */
export function describeLocator(locator: CitedValue["locator"]): string | null {
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

/** One block per source: the document, when it was retrieved, and a link to it. */
export function SourceLines({ sources }: { sources: readonly CitedSource[] }) {
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

/** The popover's content for a cited value. Phrasing content only, so it can sit inside a sentence. */
export function CitationContent({ value, currencyNamed }: { value: CitedValue; currencyNamed?: boolean }) {
  const exact = formatValue(value, "as-reported", { currencyNamed });
  const where = describeLocator(value.locator);
  return (
    <>
      <span className="popover__value">{exact}</span>
      <span className="popover__line">
        {value.label}
        {value.period ? `, ${value.period.label}` : ""}
      </span>
      {value.derived ? <span className="popover__meta">Calculated by News from the figures below.</span> : null}
      {where ? <span className="popover__meta">{where}</span> : null}
      <SourceLines sources={value.sources} />
    </>
  );
}

export interface SourcePopoverProps {
  /** Unique on the page; links the trigger and the panel. */
  id: string;
  value: CitedValue;
  /** The figure as shown in the trigger ("$391.0bn"). */
  shown: string;
  triggerClassName?: string;
  currencyNamed?: boolean;
}

/** A cited figure's trigger and its citation popover. */
export function SourcePopover({ id, value, shown, triggerClassName, currencyNamed }: SourcePopoverProps) {
  return (
    <Popover id={id} label={`Source for ${shown}`} trigger={shown} triggerClassName={triggerClassName}>
      <CitationContent value={value} currencyNamed={currencyNamed} />
    </Popover>
  );
}
