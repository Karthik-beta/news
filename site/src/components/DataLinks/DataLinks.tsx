// Downloads row (design/components.md, "Downloads row"): "CSV", "JSON" and "Markdown" as underlined text links in
// ink-2. Each link downloads; a link the caller leaves out is not shown (a licence that forbids redistribution
// gives a chart no CSV or JSON, check C8). Optional context completes each link's name for screen readers
// ("CSV, data for Net sales by region"), since a page can hold many. Static: 0 KB of JavaScript.
import { VisuallyHidden } from "../VisuallyHidden/VisuallyHidden";

export interface DataLinksProps {
  csv?: string;
  json?: string;
  markdown?: string;
  /** What the files hold, read after each link's name by screen readers only. */
  context?: string;
  className?: string;
}

export function DataLinks({ csv, json, markdown, context, className }: DataLinksProps) {
  const links = [
    { href: csv, label: "CSV" },
    { href: json, label: "JSON" },
    { href: markdown, label: "Markdown" },
  ].filter((l): l is { href: string; label: string } => Boolean(l.href));
  if (links.length === 0) return null;
  return (
    <p className={`data-links text-label${className ? ` ${className}` : ""}`}>
      {links.map((l) => (
        <a key={l.label} className="data-links__link" href={l.href} download>
          {l.label}
          {context ? <VisuallyHidden>, data for {context}</VisuallyHidden> : null}
        </a>
      ))}
    </p>
  );
}
