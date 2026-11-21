// The figure or mini chart a home slot shows, from the slot page's own data: a key figure (its label as the page
// writes it, the value with its citation popover, and the figure's sparkline when the page gives it one), or one
// of the page's charts as a mini chart (the chart's own title, its latest value and period, and a sparkline of the
// series). Values resolve against the slot page's data.json, never the home page's. Missing data says "No data".
import { PageDataProvider, usePageData } from "../../renderer/data/context";
import type { ListedPage } from "../../renderer/data/listings";
import { MISSING_TEXT } from "../../renderer/data/resolve";
import { RefText } from "../SourceRef/RefText";
import { SourceRef } from "../SourceRef/SourceRef";
import { Sparkline } from "../Sparkline/Sparkline";

export interface SlotFigureProps {
  page: ListedPage;
  /** dp_ or dv_: one of the page's key figures. */
  figure?: string | null;
  /** A chart ID: one of the page's charts. */
  chart?: string | null;
  /** "s" (figure-s) in story items, "m" (the full figure size) under the lead story. */
  size?: "s" | "m";
}

/** "No data" where the slot's figure would be: the page has no data, or the chart is not one of its charts. */
function Missing({ size }: { size: "s" | "m" }) {
  return (
    <p className={`slot-figure slot-figure--missing ${size === "m" ? "text-figure" : "text-figure-s"}`}>
      <span className="source-ref source-ref--missing">{MISSING_TEXT}</span>
    </p>
  );
}

function FigureSlot({ page, figure, size }: { page: ListedPage; figure: string; size: "s" | "m" }) {
  const data = usePageData();
  const own = page.figures?.[figure];
  const value = data.value(figure);
  const trend = own?.trend ? data.series(own.trend) : null;
  const label = own ? <RefText text={own.label} cite="plain" /> : value.label;
  return (
    <dl className="slot-figure">
      {label ? <dt className="slot-figure__label text-label">{label}</dt> : null}
      <dd className="slot-figure__value">
        <span className={`slot-figure__figure ${size === "m" ? "text-figure" : "text-figure-s"}`} data-ref={figure}>
          <SourceRef dataRef={figure} />
        </span>
        {trend?.ok ? <Sparkline series={trend} className="slot-figure__sparkline" /> : null}
      </dd>
    </dl>
  );
}

function ChartSlot({ page, chart, size }: { page: ListedPage; chart: string; size: "s" | "m" }) {
  const data = usePageData();
  const own = page.charts?.[chart];
  if (!own) return <Missing size={size} />;
  const series = data.series(own.series);
  const latest = series.ok ? series.points.findLast((p) => p.value !== null && p.ref !== null) : undefined;
  const title = <RefText text={own.title} cite="plain" />;
  if (!series.ok || !latest?.ref) {
    return (
      <dl className="slot-figure slot-figure--chart slot-figure--missing">
        <dt className="slot-figure__label text-label">{title}</dt>
        <dd className={`slot-figure__value ${size === "m" ? "text-figure" : "text-figure-s"}`}>
          <span className="source-ref source-ref--missing">{MISSING_TEXT}</span>
        </dd>
      </dl>
    );
  }
  return (
    <dl className="slot-figure slot-figure--chart" data-chart={chart}>
      <dt className="slot-figure__label text-label">{title}</dt>
      <dd className="slot-figure__value">
        <span className={`slot-figure__figure ${size === "m" ? "text-figure" : "text-figure-s"}`} data-ref={latest.ref}>
          <SourceRef dataRef={latest.ref} />
        </span>
        <Sparkline series={series} className="slot-figure__sparkline" />
      </dd>
      {latest.period.label ? <dd className="slot-figure__period">{latest.period.label}</dd> : null}
    </dl>
  );
}

export function SlotFigure({ page, figure, chart, size = "s" }: SlotFigureProps) {
  if (!figure && !chart) return null;
  if (!page.data) return <Missing size={size} />;
  return (
    <PageDataProvider resolver={page.data}>
      {figure ? (
        <FigureSlot page={page} figure={figure} size={size} />
      ) : (
        <ChartSlot page={page} chart={chart as string} size={size} />
      )}
    </PageDataProvider>
  );
}
