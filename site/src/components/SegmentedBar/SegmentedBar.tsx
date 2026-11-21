// Tall segmented bar (design/charts.md, "Choosing the form": part of a whole, at most 6 parts): one bar split into
// its parts, 28px tall, square ends, a 2px paper gap between parts, coloured by part (categorical in order, or the
// highlight and context). Under the bar, a key names every part in block order with its value as published, so no
// label has to squeeze inside a narrow block (the key is the bar's legend; dataviz "Segmented bar"). Each part is
// its own hover and focus stop. The bar is the stacked-bar island, so it adds no JavaScript of its own.
import type { ComponentRenderer } from "@openuidev/react-lang";
import { usePageData } from "../../renderer/data/context";
import { MISSING_TEXT, type PageDataResolver } from "../../renderer/data/resolve";
import { Island } from "../../renderer/islands/Island";
import { ChartFigure } from "../ChartFigure/ChartFigure";
import { subjectClass } from "../ChartFigure/interaction";
import { displayValue, type Measure, type TableModel, valueFell } from "../ChartFigure/model";
import { type StackData, stackData, stackPlot } from "../StackedBar/StackedBar";
import StackedBarPlot from "../StackedBar/StackedBarPlot.client";

export interface SegmentInput {
  label: string;
  value: string;
  highlight?: boolean | null;
}

export interface SegmentedBarProps {
  title: string;
  subtitle: string;
  parts: SegmentInput[];
}

/**
 * The parts as one stacked row; the table turned so each part is a row (a whole read part by part). A highlighted
 * part below zero takes the neutral highlight (the bar rule); a part below zero cannot be drawn and is in the key
 * and table only.
 */
export function segmentedBarData(
  props: SegmentedBarProps,
  data: PageDataResolver,
): { stack: StackData; model: TableModel | null } {
  const subject = props.parts.find((p) => p.highlight) ?? null;
  const highlight = subject?.label ?? null;
  const resolved = subject ? data.value(subject.value) : null;
  const fell = resolved?.ok ? valueFell(resolved.value) : false;
  const stack = stackData(
    props.parts.map((p) => p.label),
    [{ label: data.plainText(props.title), values: props.parts.map((p) => p.value) }],
    {
      title: data.plainText(props.title),
      subtitle: data.plainText(props.subtitle),
      highlight,
      head: "Part",
      fell,
    },
    data,
  );
  const wide = stack.model;
  const cells = stack.rows[0]?.cells ?? [];
  const model: TableModel | null =
    wide && stack.measure
      ? {
          ...wide,
          head: "Part",
          columns: [
            { key: "value", label: periodLabel(cells.map((c) => c.value?.period?.label ?? "")), ...stack.measure },
          ],
          rows: props.parts.map((p, i) => ({
            key: `part-${i}`,
            label: p.label,
            highlight: Boolean(p.highlight),
            fell: Boolean(p.highlight) && stack.roles[i] === "highlight-neutral",
            cells: [{ ref: p.value, value: cells[i]?.value?.value ?? null }],
          })),
        }
      : null;
  return { stack, model };
}

/** One period shared by every part names the value column ("fiscal 2024"); otherwise "Value". */
const periodLabel = (labels: readonly string[]) => {
  const set = new Set(labels.filter(Boolean));
  return set.size === 1 ? ([...set][0] as string) : "Value";
};

export const SegmentedBar: ComponentRenderer<SegmentedBarProps> = ({ props, statementId }) => {
  const data = usePageData();
  const { stack, model } = segmentedBarData(props, data);
  const cells = stack.rows[0]?.cells ?? [];
  const plotted = cells.some((c) => c.value !== null);
  const measure = stack.measure as Measure;
  return (
    <ChartFigure statementId={statementId} title={props.title} subtitle={props.subtitle} model={model}>
      {plotted ? (
        <div className="segmented-bar">
          <Island name="StackedBarPlot" component={StackedBarPlot} props={stackPlot(stack, "segmented", data)} />
          <ul className="segmented-bar__key">
            {props.parts.map((p, i) => {
              const c = cells[i];
              const role = stack.roles[i] ?? "cat-1";
              return (
                <li
                  key={`${p.value}-${p.label}`}
                  className={`segmented-bar__item${p.highlight ? " is-highlight" : ""}`}
                >
                  <span className={`chart-key chart-key--square chart-colour--${role}`} aria-hidden="true" />
                  <span className="segmented-bar__name">{p.label}</span>
                  <span className={`segmented-bar__value text-tabular${subjectClass(role)}`} data-ref={p.value}>
                    {c?.value ? displayValue(c.value.value, measure) : MISSING_TEXT}
                  </span>
                </li>
              );
            })}
          </ul>
        </div>
      ) : null}
    </ChartFigure>
  );
};
