import { z } from "zod/v4";
import { refPattern } from "../../renderer/bundle/schema";
import { MISSING_TEXT } from "../../renderer/data/resolve";
import { escapeMarkdown } from "../../renderer/markdown";
import { defineNewsComponent } from "../contract";
import { Quote, quoteParts } from "./Quote";

export default defineNewsComponent({
  kind: "component",
  name: "Quote",
  index: "Quote — short primary-source quote, attributed — q_ ref",
  description:
    "A short quote (one to three sentences) from a primary source, attributed and linked from data; the text comes from the q_ reference.",
  dataShape: "quote: a q_… reference; speaker, role, document and date come from data.json and sources.json.",
  example: `statement = Quote("q_fomc_statement_sep_2024")`,
  props: z.object({
    quote: z.string().regex(refPattern("q")).describe("q_… reference"),
  }),
  component: Quote,
  markdown: ({ props, data }) => {
    const parts = quoteParts(props.quote, data);
    if (!parts) return MISSING_TEXT;
    const by = parts.attribution
      ? `\n>\n> — ${parts.url ? `[${escapeMarkdown(parts.attribution)}](${parts.url})` : escapeMarkdown(parts.attribution)}`
      : "";
    return `> “${escapeMarkdown(parts.text)}”${by}`;
  },
});
