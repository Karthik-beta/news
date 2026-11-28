// Steps (design/components.md, "Steps"): how something works, in order, for explainers: the house version of
// react-ui's Steps and the one "diagram" an explainer needs without drawing. An ordered list; each step has its
// number in Fira 600 (worked out by code from the order), a short title in Fira 600 and one or two sentences in
// Literata with {ref} tokens. A 1px rule joins the numbers. One source line under the list for every value and
// quote the steps cite. Static: 0 KB JS.
import type { ComponentRenderer } from "@openuidev/react-lang";
import { usePageData } from "../../renderer/data/context";
import { SourceLine } from "../ChartFigure/ChartFigure";
import { sourcesOf } from "../ChartFigure/model";
import { CopyText } from "../Prose/copy";
import { textSources } from "../Timeline/sources";

export interface Step {
  /** A few words naming the step. */
  title: string;
  /** What happens, with {ref} tokens. */
  text: string;
}

export interface StepsProps {
  steps: Step[];
}

export const Steps: ComponentRenderer<StepsProps> = ({ props }) => {
  const data = usePageData();
  if (props.steps.length === 0) return null;
  const sources = sourcesOf(...props.steps.map((s) => textSources(data, s.text)));
  return (
    <div className="steps">
      <ol className="steps__list">
        {props.steps.map((step, i) => (
          // biome-ignore lint/suspicious/noArrayIndexKey: steps may share a title; the order is fixed
          <li key={i} className="steps__step">
            <span className="steps__number" aria-hidden="true">
              {i + 1}
            </span>
            <div className="steps__body">
              <p className="steps__title">{data.plainText(step.title)}</p>
              <p className="steps__text">
                <CopyText text={step.text} />
              </p>
            </div>
          </li>
        ))}
      </ol>
      <SourceLine sources={sources} className="steps__source" />
    </div>
  );
};
