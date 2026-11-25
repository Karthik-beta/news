import { z } from "zod/v4";
import { defineNewsComponent } from "../contract";
import { copyMarkdown, parseBlocks } from "./copy";
import { Prose } from "./Prose";

export default defineNewsComponent({
  kind: "component",
  name: "Prose",
  index: "Prose — article text, ## headings, - lists — blocks[], {ref}s",
  description:
    "Article text: one string per paragraph; '## ' starts a section heading, '- ' lines make a list; numbers only as {ref} tokens; mark glossary terms as [words](glossary:slug).",
  dataShape:
    "blocks: strings with {dp_…}, {dv_…}, {pd_…}, {cx_…} or {q_…} tokens and no typed numbers; [words](glossary:slug) links a glossary term.",
  example: `intro = Prose(["## Why sales rose", "Net sales rose to {dp_aapl_net_sales_fy2024} in {pd_fy2024}, {cx_aapl_first_rise_2y}.", "- Services grew fastest\\n- [Gross margin](glossary:gross-margin) widened"])`,
  props: z.object({
    blocks: z
      .array(z.string())
      .min(1)
      .describe("One string per paragraph, '## heading' or '- item' lines; numbers only as {ref} tokens"),
  }),
  component: Prose,
  markdown: ({ props, data }) =>
    parseBlocks(props.blocks)
      .map((block) => {
        if (block.type === "heading") return `## ${copyMarkdown(block.text, data)}`;
        if (block.type === "list") return block.items.map((item) => `- ${copyMarkdown(item, data)}`).join("\n");
        return copyMarkdown(block.text, data);
      })
      .join("\n\n"),
});
