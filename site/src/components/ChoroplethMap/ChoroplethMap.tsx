// Choropleth map (design/charts.md, "Maps"): countries on an Equal Earth world map, each shaded by its value on the
// sequential blue ramp in the heatmap's steps (validated in both themes), up to five classes broken at round numbers
// near the quintiles (classes.ts). Borders are paper-coloured hairlines; a country without data is
// hatched in the neutral colour, and the legend says "No data". The map is always paired with its data ranked: a
// table beside it, or, when the plot is narrow (every phone; a container query, like the Sankey's fallback), the
// bar chart's ranked bars under a small map. The map is static SVG drawn on the server from geometry projected at
// build time (scripts/gen-map-geometry.ts), so it ships no JavaScript; the bars are the bar chart's own island.
import type { ComponentRenderer } from "@openuidev/react-lang";
import { useId } from "react";
import { usePageData } from "../../renderer/data/context";
import { MISSING_TEXT, type PageDataResolver } from "../../renderer/data/resolve";
import { Island } from "../../renderer/islands/Island";
import { type BarChartData, barChartData, barPlot } from "../BarChart/BarChart";
import BarChartPlot from "../BarChart/BarChartPlot.client";
import { ChartFigure, hasValues } from "../ChartFigure/ChartFigure";
import { modelSummary, type TableModel } from "../ChartFigure/model";
import { displayExponent } from "../ChartFigure/scale";
import { TableView } from "../DataTable/TableView";
import { VisuallyHidden } from "../VisuallyHidden/VisuallyHidden";
import { classText, type MapClasses, mapClasses } from "./classes";
import { world } from "./geo/world";

export interface RegionInput {
  /** ISO 3166-1 alpha-3 code ("FRA"), as in geo/world.ts. */
  region: string;
  /** The country's name as the page writes it. */
  label: string;
  value: string;
}

export interface ChoroplethMapProps {
  title: string;
  subtitle: string;
  regions: RegionInput[];
}

export interface ChoroplethData extends BarChartData {
  /** The ramp step (1-5) and data reference of each region with a value, by code; regions not here are hatched. */
  steps: Map<string, { step: number; ref: string }>;
  classes: MapClasses | null;
}

/** What the rows of a map's table are, for the accessible summary. */
const ROWS_ARE = "countries";

/**
 * The ranked data (the bar chart's: values on one axis, largest first, the table model) with the class of each
 * value. The table's rows and the map's shaded regions come from the same list, so they can never disagree.
 */
export function choroplethData(props: ChoroplethMapProps, data: PageDataResolver): ChoroplethData {
  const chart = barChartData({ title: props.title, subtitle: props.subtitle, bars: props.regions }, data);
  const model: TableModel | null = chart.model ? { ...chart.model, head: "Country" } : null;
  const steps: ChoroplethData["steps"] = new Map();
  const values = chart.bars.flatMap((b) => (b.value ? [b.value.value] : []));
  if (!chart.measure || values.length === 0) return { ...chart, model, steps, classes: null };
  const classes = mapClasses(values, chart.measure, displayExponent(values, chart.measure));
  for (const b of chart.bars) {
    const step = b.value ? classes.steps[classes.classOf(b.value.value)] : undefined;
    if (step !== undefined) steps.set((b.input as RegionInput).region, { step, ref: b.input.value });
  }
  return { ...chart, model, steps, classes };
}

/** The legend bar above the map: one swatch per class, each break under its join, then "No data". */
function MapLegend({ classes, missing }: { classes: MapClasses; missing: boolean }) {
  const { joins, steps, only } = classes;
  return (
    <div className="chart-legend choropleth__legend text-label">
      <ol className="choropleth__scale">
        {steps.map((step, i) => (
          <li key={step} className="choropleth__class">
            <span className={`choropleth__swatch heatmap__step--${step}`} aria-hidden="true" />
            {joins[i] || only !== null ? (
              <span
                className={`choropleth__edge text-tabular${only === null ? " choropleth__edge--join" : ""}`}
                aria-hidden="true"
              >
                {joins[i]?.label ?? only}
              </span>
            ) : null}
            <VisuallyHidden>{classText(classes, i)}</VisuallyHidden>
          </li>
        ))}
      </ol>
      {missing ? (
        <span className="chart-legend__item">
          <span className="chart-key chart-key--square chart-hatch" aria-hidden="true" />
          {MISSING_TEXT}
        </span>
      ) : null}
    </div>
  );
}

const regions = world.regions;

/** The world map: every country in the geometry, shaded by its step or hatched. */
function WorldMap({ steps, summary }: { steps: ChoroplethData["steps"]; summary: string }) {
  const hatch = `hatch-${useId().replace(/[^a-zA-Z0-9_-]/g, "")}`;
  return (
    <svg className="choropleth__map" viewBox={world.viewBox} role="img" aria-label={summary}>
      <defs>
        <pattern id={hatch} width="6" height="6" patternUnits="userSpaceOnUse" patternTransform="rotate(45)">
          <line className="choropleth__hatch" x1="0" y1="0" x2="0" y2="6" />
        </pattern>
      </defs>
      {Object.entries(regions).map(([code, d]) => {
        const shade = steps.get(code);
        return shade === undefined ? (
          <path key={code} d={d} className="choropleth__region is-missing" fill={`url(#${hatch})`} data-region={code} />
        ) : (
          <path
            key={code}
            d={d}
            className={`choropleth__region heatmap__step--${shade.step}`}
            data-region={code}
            data-ref={shade.ref}
          />
        );
      })}
    </svg>
  );
}

export const ChoroplethMap: ComponentRenderer<ChoroplethMapProps> = ({ props, statementId }) => {
  const data = usePageData();
  const chart = choroplethData(props, data);
  const model = chart.model;
  let body = null;
  if (hasValues(model) && chart.classes) {
    const summary = modelSummary(model, ROWS_ARE);
    const missing = Object.keys(regions).some((code) => !chart.steps.has(code));
    // The map carries the summary; the ranked table or bars are named "Ranked: <title>", so a screen reader neither
    // hears the summary twice nor finds two tables of the same name once "Show the data table" is open.
    const rankedId = statementId ? `${statementId}-ranked` : undefined;
    body = (
      <div className="choropleth">
        <MapLegend classes={chart.classes} missing={missing} />
        <WorldMap steps={chart.steps} summary={summary} />
        <div className="choropleth__table">
          {rankedId ? (
            <span id={rankedId} hidden>
              Ranked:
            </span>
          ) : null}
          <TableView model={model} labelledBy={rankedId ? `${rankedId} ${statementId}-title` : undefined} />
        </div>
        <div className="choropleth__bars">
          <Island
            name="BarChartPlot"
            component={BarChartPlot}
            props={{ ...barPlot(chart, data), summary: `Ranked: ${model.title}` }}
          />
        </div>
      </div>
    );
  }
  return (
    <ChartFigure statementId={statementId} title={props.title} subtitle={props.subtitle} model={model}>
      {body}
    </ChartFigure>
  );
};
