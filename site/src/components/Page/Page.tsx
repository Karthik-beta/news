import type { ComponentRenderer } from "@openuidev/react-lang";

export interface PageProps {
  children: unknown[];
}

/**
 * The root of every page.oui: the blocks between the article head and the sources, in reading order. The page
 * template around it (head, sources panel, rail) belongs to the renderer and the article components.
 */
export const Page: ComponentRenderer<PageProps> = ({ props, renderNode }) => (
  <div className="page-body">{renderNode(props.children)}</div>
);
