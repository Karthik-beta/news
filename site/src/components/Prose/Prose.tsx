// Article prose (design/components.md, "Prose"): body type at most 66 characters a line, 14px between paragraphs,
// section headings in headline-l with 28px above and 10px below, plain disc lists indented 20px. Static: 0 KB JS.
// Numbers only arrive as {ref} tokens and are cited through SourceRef; glossary terms open their definition.
import type { ComponentRenderer } from "@openuidev/react-lang";
import { usePageData } from "../../renderer/data/context";
import { type Block, CopyText, headingId, parseBlocks } from "./copy";

export interface ProseProps {
  blocks: string[];
}

function ProseBlock({ block }: { block: Block }) {
  const data = usePageData();
  switch (block.type) {
    case "heading":
      return (
        <h2 className="prose__heading text-headline-l" id={headingId(block.text, data)}>
          <CopyText text={block.text} cite="plain" />
        </h2>
      );
    case "list":
      return (
        <ul className="prose__list">
          {block.items.map((item, i) => (
            // biome-ignore lint/suspicious/noArrayIndexKey: list items are static copy in a fixed order
            <li key={i}>
              <CopyText text={item} />
            </li>
          ))}
        </ul>
      );
    default:
      return (
        <p>
          <CopyText text={block.text} />
        </p>
      );
  }
}

export const Prose: ComponentRenderer<ProseProps> = ({ props }) => {
  const blocks = parseBlocks(props.blocks);
  if (blocks.length === 0) return null;
  return (
    <div className="prose text-body">
      {blocks.map((block, i) => (
        // biome-ignore lint/suspicious/noArrayIndexKey: blocks are static copy in a fixed order
        <ProseBlock key={i} block={block} />
      ))}
    </div>
  );
};
