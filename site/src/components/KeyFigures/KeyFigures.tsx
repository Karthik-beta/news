// Key figures (design/components.md, "Key figures"): two to four headline numbers in a row (two across on a phone),
// each a KeyFigure, with one source line under the whole row. A description list, so screen readers hear each
// label with its value and change (design/accessibility.md, "Structure"). Static: 0 KB of JavaScript.
import type { ComponentRenderer, ElementNode } from "@openuidev/react-lang";
import { usePageData } from "../../renderer/data/context";
import { SourceLine } from "../ChartFigure/ChartFigure";
import { sourcesOf } from "../ChartFigure/model";
import { figureSources, type KeyFigureProps, KeyFiguresRow } from "../KeyFigure/KeyFigure";

export interface KeyFiguresProps {
  /** KeyFigure elements, in reading order. */
  children: unknown[];
}

const isFigure = (v: unknown): v is ElementNode & { props: KeyFigureProps } =>
  typeof v === "object" &&
  v !== null &&
  (v as ElementNode).type === "element" &&
  (v as ElementNode).typeName === "KeyFigure";

export const KeyFigures: ComponentRenderer<KeyFiguresProps> = ({ props, renderNode }) => {
  const data = usePageData();
  const figures = props.children.filter(isFigure);
  const sources = sourcesOf(...figures.map((f) => figureSources(data, f.props)));
  return (
    <div className={`key-figures key-figures--n${figures.length}`}>
      <dl className="key-figures__list">
        <KeyFiguresRow>{renderNode(figures)}</KeyFiguresRow>
      </dl>
      <SourceLine sources={sources} className="key-figures__source" />
    </div>
  );
};
