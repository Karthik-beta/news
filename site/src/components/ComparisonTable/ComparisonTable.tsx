// Comparison table (design/components.md, "Comparison table"): guidance against actual, this quarter against the
// last, one country against another. The two compared columns sit side by side and the change comes last, as a
// signed figure (or in words), never coloured. The same table style as DataTable, with the chart anatomy around
// it. Values are written compact, each in its own unit; every value cell carries its reference. Static: 0 KB JS.
import type { ComponentRenderer } from "@openuidev/react-lang";
import { usePageData } from "../../renderer/data/context";
import { MISSING_TEXT, type PageDataResolver, type ResolvedValue } from "../../renderer/data/resolve";
import { downloadsFor } from "../../renderer/exports";
import { useFigureId } from "../ChartFigure/ChartFigure";
import { displayValue, sourcesOf, unitLabel } from "../ChartFigure/model";
import type { ChartTable } from "../contract";
import { TableScroll } from "../DataTable/DataTable";
import { FigureFrame } from "../FigureFrame/FigureFrame";
import { changeText } from "../KeyFigure/change";
import { RefText } from "../SourceRef/RefText";

export interface ComparisonRowInput {
  label: string;
  a: string;
  b: string;
  change?: string | null;
  highlight?: boolean | null;
}

export interface ComparisonTableProps {
  title: string;
  subtitle: string;
  /** The two compared columns' headings, as text (may hold {pd_} tokens). */
  columns: string[];
  rows: ComparisonRowInput[];
  /** "signed" (the default): "+7.7%", "−7.7%". "words": "Up 7.7%", "Down 7.7%". */
  changeStyle?: "signed" | "words" | null;
}

type OkValue = Extract<ResolvedValue, { ok: true }>;

export interface ComparisonRow {
  input: ComparisonRowInput;
  a: OkValue | null;
  b: OkValue | null;
  change: OkValue | null;
}

export interface ComparisonData {
  headings: [string, string];
  rows: ComparisonRow[];
  hasChange: boolean;
}

const ok = (v: ResolvedValue): OkValue | null => (v.ok ? v : null);

export function comparisonData(props: ComparisonTableProps, data: PageDataResolver): ComparisonData {
  const rows = props.rows.map((r) => ({
    input: r,
    a: ok(data.value(r.a)),
    b: ok(data.value(r.b)),
    change: r.change ? ok(data.value(r.change)) : null,
  }));
  return {
    headings: [data.plainText(props.columns[0] ?? ""), data.plainText(props.columns[1] ?? "")],
    rows,
    hasChange: props.rows.some((r) => Boolean(r.change)),
  };
}

const sourcesOfRows = (rows: readonly ComparisonRow[]) =>
  sourcesOf(...rows.flatMap((r) => [r.a, r.b, r.change].flatMap((v) => (v ? [v.sources] : []))));

/** The unit of a row, from its first value: "US$ millions", "%". */
const rowUnit = (r: ComparisonRow) => {
  const v = r.a ?? r.b;
  return v ? unitLabel(v) : "";
};

/** The download: exact values, verbatim labels, the change as calculated, and each row's unit. */
export function comparisonCsv(props: ComparisonTableProps, data: PageDataResolver): ChartTable {
  const c = comparisonData(props, data);
  return {
    title: data.plainText(props.title),
    subtitle: data.plainText(props.subtitle),
    columns: [
      { key: "label", label: "Measure" },
      { key: "a", label: c.headings[0] },
      { key: "b", label: c.headings[1] },
      ...(c.hasChange ? [{ key: "change", label: "Change" }] : []),
      { key: "unit", label: "Unit" },
    ],
    rows: c.rows.map((r) => ({
      label: r.input.label,
      a: r.a?.value ?? null,
      b: r.b?.value ?? null,
      ...(c.hasChange ? { change: r.change?.value ?? null } : {}),
      unit: rowUnit(r) || null,
    })),
    sources: sourcesOfRows(c.rows).map((s) => s.id),
  };
}

export const ComparisonTable: ComponentRenderer<ComparisonTableProps> = ({ props, statementId }) => {
  const data = usePageData();
  const id = useFigureId(statementId);
  const c = comparisonData(props, data);
  const sources = sourcesOfRows(c.rows);
  const titleId = `${id}-title`;
  const style = props.changeStyle ?? "signed";
  const cell = (v: OkValue | null, ref: string) => (
    <td className={v ? "data-table__cell is-number" : "data-table__cell is-number is-missing"} data-ref={ref}>
      {v ? displayValue(v.value, v) : MISSING_TEXT}
    </td>
  );
  return (
    <FigureFrame
      id={id}
      title={<RefText text={props.title} cite="plain" />}
      subtitle={props.subtitle ? <RefText text={props.subtitle} cite="plain" /> : undefined}
      sources={sources}
      downloads={downloadsFor(statementId, sources)}
    >
      <TableScroll labelledBy={titleId}>
        <table className="data-table data-table--compact comparison-table" aria-labelledby={titleId}>
          <thead>
            <tr>
              <th scope="col" className="data-table__head">
                Measure
              </th>
              <th scope="col" className="data-table__head is-number">
                <RefText text={props.columns[0] ?? ""} cite="plain" />
              </th>
              <th scope="col" className="data-table__head is-number">
                <RefText text={props.columns[1] ?? ""} cite="plain" />
              </th>
              {c.hasChange ? (
                <th scope="col" className="data-table__head is-number">
                  Change
                </th>
              ) : null}
            </tr>
          </thead>
          <tbody>
            {c.rows.map((r) => (
              <tr
                key={`${r.input.label}-${r.input.a}-${r.input.b}`}
                className={r.input.highlight ? "data-table__row is-highlight" : "data-table__row"}
              >
                <th scope="row" className="data-table__row-head">
                  {r.input.label}
                </th>
                {cell(r.a, r.input.a)}
                {cell(r.b, r.input.b)}
                {c.hasChange ? (
                  <td
                    className={`data-table__cell is-number comparison-table__change${r.change ? "" : " is-missing"}`}
                    data-ref={r.input.change ?? undefined}
                  >
                    {r.change ? changeText(r.change, style) : MISSING_TEXT}
                  </td>
                ) : null}
              </tr>
            ))}
          </tbody>
        </table>
      </TableScroll>
    </FigureFrame>
  );
};
