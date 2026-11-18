// Chart and table downloads: charts/<statement>.csv and .json next to index.html. Values are exact source values
// (decimal strings, verbatim labels), never formatted. A source whose licence does not allow redistribution
// blocks its downloads (check C8), and the chart then shows no CSV or JSON link.
import type { ChartTable } from "../components/contract";
import type { PageSourcesT } from "./bundle/schema";
import type { CitedSource } from "./data/resolve";
import { RENDERER_NAME, RENDERER_VERSION } from "./version";

export const chartFile = (statementId: string, ext: "csv" | "json") => `charts/${statementId}.${ext}`;

/** True when every source may be redistributed. */
export function mayRedistribute(sources: readonly CitedSource[]): boolean {
  return sources.length > 0 && sources.every(({ source }) => source.licence.redistribution);
}

/** The download links a chart shows: relative to the page, or none when a licence forbids it. */
export function downloadsFor(
  statementId: string | undefined,
  sources: readonly CitedSource[],
): { csv: string; json: string } | undefined {
  if (!statementId || !mayRedistribute(sources)) return undefined;
  return { csv: chartFile(statementId, "csv"), json: chartFile(statementId, "json") };
}

const DECIMAL = /^-?(0|[1-9][0-9]*)(\.[0-9]+)?$/;

/** One CSV field (RFC 4180). Text that a spreadsheet would run as a formula gets a leading apostrophe. */
export function csvField(value: string | null): string {
  if (value === null) return "";
  let v = value;
  if (!DECIMAL.test(v) && /^[=+\-@\t\r]/.test(v)) v = `'${v}`;
  return /[",\r\n]/.test(v) ? `"${v.replaceAll('"', '""')}"` : v;
}

/** RFC 4180 CSV: a header row of column labels, then one row per data row, CRLF line endings. */
export function toCsv(table: ChartTable): string {
  const lines = [table.columns.map((c) => csvField(c.label)).join(",")];
  for (const row of table.rows) lines.push(table.columns.map((c) => csvField(row[c.key] ?? null)).join(","));
  return `${lines.join("\r\n")}\r\n`;
}

/** The JSON download: the table plus its unit, currency, scale and sources, so it stands on its own. */
export function toChartJson(table: ChartTable, sources: PageSourcesT): string {
  const cited = table.sources.map((id) => {
    const s = sources.sources[id];
    return s
      ? {
          id,
          title: s.title,
          publisher: s.publisher,
          url: s.url,
          licence: s.licence.name,
          retrieved_at: s.retrieved_at,
        }
      : { id };
  });
  const json = {
    title: table.title,
    subtitle: table.subtitle ?? null,
    unit: table.unit ?? null,
    currency: table.currency ?? null,
    scale: table.scale ?? null,
    columns: table.columns,
    rows: table.rows,
    sources: cited,
    generator: `${RENDERER_NAME} ${RENDERER_VERSION}`,
  };
  return `${JSON.stringify(json, null, 2)}\n`;
}
