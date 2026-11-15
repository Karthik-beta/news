// The frame every chart and table sits in (design/charts.md, "Anatomy"): brand rule and tag, takeaway title,
// subtitle with the unit, legend, plot, source line, then actions ("Show the data table", "CSV", "JSON").
// Static: it ships no JavaScript. The data table toggle is a native <details>, so it works with JavaScript off.
import type { ReactNode } from "react";
import type { CitedSource } from "../../renderer/data/resolve";
import { ruleTop } from "../Rule/Rule";

export interface FigureFrameProps {
  /** Used for the title's id and the downloads' file names. Unique on the page. */
  id: string;
  /** The takeaway, as a sentence with no full stop. */
  title: ReactNode;
  /** What is plotted, the period and the unit. */
  subtitle?: ReactNode;
  /** Only for two or more series. */
  legend?: ReactNode;
  /** The plot (or the table, for tables as charts). */
  children: ReactNode;
  /** Sources behind the figure; linked to their entries in the page's sources list. */
  sources?: CitedSource[];
  /** A line before the sources ("Calculated by News from …"). */
  sourceNote?: ReactNode;
  /** The data as a real table, behind "Show the data table". */
  table?: ReactNode;
  /** Download links; left out when the source's licence does not allow redistribution (check C8). */
  downloads?: { csv?: string; json?: string };
  /** The heading level of the title: 2 in articles (design/accessibility.md). */
  headingLevel?: 2 | 3 | 4;
}

export const sourceAnchor = (id: string) => `source-${id}`;

export function FigureFrame({
  id,
  title,
  subtitle,
  legend,
  children,
  sources = [],
  sourceNote,
  table,
  downloads,
  headingLevel = 2,
}: FigureFrameProps) {
  const Heading = `h${headingLevel}` as "h2";
  const titleId = `${id}-title`;
  const hasActions = Boolean(table || downloads?.csv || downloads?.json);
  return (
    <figure className={`figure-frame ${ruleTop("brand")}`} aria-labelledby={titleId}>
      <figcaption className="figure-frame__caption">
        <Heading className="figure-frame__title text-chart-title" id={titleId}>
          {title}
        </Heading>
        {subtitle ? <p className="figure-frame__subtitle text-chart-subtitle">{subtitle}</p> : null}
      </figcaption>
      {legend ? <div className="figure-frame__legend">{legend}</div> : null}
      <div className="figure-frame__plot">{children}</div>
      {sourceNote || sources.length > 0 ? (
        <p className="figure-frame__source text-source">
          {sourceNote ? <>{sourceNote} </> : null}
          {sources.length > 0 ? (
            <>
              {sources.length > 1 ? "Sources: " : "Source: "}
              {sources.map(({ id: sourceId, source }, i) => (
                <span key={sourceId}>
                  {i > 0 ? "; " : ""}
                  <a href={`#${sourceAnchor(sourceId)}`}>
                    {source.publisher}, {source.short}
                  </a>
                </span>
              ))}
            </>
          ) : null}
        </p>
      ) : null}
      {hasActions ? (
        <div className="figure-frame__actions text-label">
          {table ? (
            <details className="figure-frame__table">
              <summary>Show the data table</summary>
              <div className="figure-frame__table-scroll">{table}</div>
            </details>
          ) : null}
          {downloads?.csv ? (
            <a href={downloads.csv} download>
              CSV
            </a>
          ) : null}
          {downloads?.json ? (
            <a href={downloads.json} download>
              JSON
            </a>
          ) : null}
        </div>
      ) : null}
    </figure>
  );
}
