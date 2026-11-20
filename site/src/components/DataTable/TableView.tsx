// A data table drawn from a table model (design/charts.md, "Tables as charts"): numbers right-aligned in tabular
// figures, text left-aligned, the header in secondary ink over a 1px ink rule, hairlines between rows, no zebra
// stripes and no vertical lines. The first column stays put while a wide table scrolls. The highlighted row is
// bold with a 3px crimson bar on its left, or a neutral-highlight bar when its subject fell. Every value cell carries its data reference. Static: no JavaScript.
import { formatCell, type TableModel, type TableStyle, unitLabel } from "../ChartFigure/model";

export interface TableViewProps {
  model: TableModel;
  /** As reported (the default): the source's digits, with the unit under each heading. Compact: "$391.0bn". */
  style?: TableStyle;
  /** The id of the heading that names the table (the figure's title). */
  labelledBy?: string;
}

export function TableView({ model, style = "as-reported", labelledBy }: TableViewProps) {
  return (
    <table className={`data-table data-table--${style}`} aria-labelledby={labelledBy}>
      <thead>
        <tr>
          <th scope="col" className="data-table__head">
            {model.head}
          </th>
          {model.columns.map((c) => (
            <th key={c.key} scope="col" className="data-table__head is-number">
              <span className="data-table__label">{c.label}</span>
              {style === "as-reported" ? <span className="data-table__unit">{unitLabel(c)}</span> : null}
            </th>
          ))}
        </tr>
      </thead>
      <tbody>
        {model.rows.map((r) => (
          <tr
            key={r.key}
            className={`data-table__row${r.highlight ? " is-highlight" : ""}${r.highlight && r.fell ? " is-fell" : ""}`}
          >
            <th scope="row" className="data-table__row-head">
              {r.label}
            </th>
            {r.cells.map((cell, i) => {
              const column = model.columns[i];
              if (!column) return null;
              return (
                <td
                  key={column.key}
                  className={
                    cell.value === null ? "data-table__cell is-number is-missing" : "data-table__cell is-number"
                  }
                  data-ref={cell.ref ?? undefined}
                >
                  {formatCell(cell.value, column, style)}
                </td>
              );
            })}
          </tr>
        ))}
      </tbody>
    </table>
  );
}
