// How a study or major report was done, as a short definition list (design/components.md, "Method summary"):
// Source type, Study design, Sample, Period, and Limitations as stated by the authors. Labels in Fira 13.5,
// secondary ink; values in Literata. A row the writer leaves out is not shown. Static: 0 KB JS.
import type { ComponentRenderer } from "@openuidev/react-lang";
import { CopyText } from "../Prose/copy";

export interface MethodSummaryProps {
  sourceType?: string | null;
  design?: string | null;
  sample?: string | null;
  period?: string | null;
  limitations?: string | null;
}

/** The rows in their fixed order, with the labels the reader sees. */
export const METHOD_ROWS = [
  ["sourceType", "Source type"],
  ["design", "Study design"],
  ["sample", "Sample"],
  ["period", "Period"],
  ["limitations", "Limitations as stated by the authors"],
] as const satisfies readonly (readonly [keyof MethodSummaryProps, string])[];

export const methodRows = (props: MethodSummaryProps) =>
  METHOD_ROWS.flatMap(([key, label]) => {
    const value = props[key];
    return typeof value === "string" && value.trim() !== "" ? [{ key, label, value: value.trim() }] : [];
  });

export const MethodSummary: ComponentRenderer<MethodSummaryProps> = ({ props }) => {
  const rows = methodRows(props);
  if (rows.length === 0) return null;
  return (
    <dl className="method-summary">
      {rows.map((row) => (
        <div key={row.key} className="method-summary__row">
          <dt className="method-summary__label text-label">{row.label}</dt>
          <dd className="method-summary__value text-body-s">
            <CopyText text={row.value} />
          </dd>
        </div>
      ))}
    </dl>
  );
};
