// A timeline (design/components.md, "Timeline"): a vertical list of dated events. Each event: the date in Fira 600
// (the period's label exactly as the source writes it), in a 7em column on wider screens and above the text on
// phones; the event in Literata; a source line when the text cites a value or a quote. A 1px rule joins the
// events; each has an 8px ink dot, and the key event's dot is crimson with a ring (so the key event is not told
// by colour alone; screen readers hear "Key event"). Horizontal timelines live only inside charts. Static.
import type { ComponentRenderer } from "@openuidev/react-lang";
import { usePageData } from "../../renderer/data/context";
import { MISSING_TEXT } from "../../renderer/data/resolve";
import { SourceLine } from "../ChartFigure/ChartFigure";
import { CopyText } from "../Prose/copy";
import { VisuallyHidden } from "../VisuallyHidden/VisuallyHidden";
import { textSources } from "./sources";

export interface TimelineEvent {
  /** pd_… the event's date or period. */
  date: string;
  /** What happened, with {ref} tokens. */
  text: string;
}

export interface TimelineProps {
  events: TimelineEvent[];
  /** pd_… of the key event (crimson dot). */
  highlight?: string | null;
}

export const Timeline: ComponentRenderer<TimelineProps> = ({ props }) => {
  const data = usePageData();
  if (props.events.length === 0) return null;
  // One key event at most: when two events share the highlighted date, the first is the key one.
  const keyIndex = props.highlight == null ? -1 : props.events.findIndex((e) => e.date === props.highlight);
  return (
    <ol className="timeline">
      {props.events.map((event, i) => {
        const period = data.period(event.date);
        const key = i === keyIndex;
        const sources = textSources(data, event.text);
        return (
          <li
            // biome-ignore lint/suspicious/noArrayIndexKey: two events may share a date; order is fixed
            key={`${i}-${event.date}`}
            className={`timeline__event${key ? " timeline__event--key" : ""}`}
          >
            <p className="timeline__date" data-ref={event.date}>
              {key ? <VisuallyHidden>Key event: </VisuallyHidden> : null}
              {period ? (
                period.period.start ? (
                  <time dateTime={period.period.start}>{period.label}</time>
                ) : (
                  period.label
                )
              ) : (
                <span className="source-ref--missing">{MISSING_TEXT}</span>
              )}
            </p>
            <div className="timeline__body">
              <p className="timeline__text">
                <CopyText text={event.text} />
              </p>
              <SourceLine sources={sources} className="timeline__source" />
            </div>
          </li>
        );
      })}
    </ol>
  );
};
