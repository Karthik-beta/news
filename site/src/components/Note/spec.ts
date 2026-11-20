import { z } from "zod/v4";
import { escapeMarkdown } from "../../renderer/markdown";
import { defineNewsComponent } from "../contract";
import { copyMarkdown, copyPlainText, parseBlocks } from "../Prose/copy";
import { Note, noteAttribution, noteSources } from "./Note";

export default defineNewsComponent({
  kind: "component",
  name: "Note",
  index: "Note — context in Claude's words, max two — heading, paragraphs[]",
  description:
    "A tinted note explaining context in Claude's own words; the heading says what it answers; never predicts, recommends or judges; at most two per page.",
  dataShape:
    "heading: plain text; paragraphs: strings with {ref} tokens for every number; the attribution line is written by code from the cited sources.",
  example: `why = Note("Why net income fell while sales rose", ["A one-off tax charge of {dp_aapl_tax_charge_fy2024} reduced net income in {pd_fy2024}."])`,
  props: z.object({
    heading: z.string().min(1).describe("What the note answers, as a phrase"),
    paragraphs: z.array(z.string()).min(1).describe("Paragraphs; numbers only as {ref} tokens"),
  }),
  component: Note,
  markdown: ({ props, data }) => {
    const paragraphs = parseBlocks(props.paragraphs).map((b) => (b.type === "list" ? b.items.join(" ") : b.text));
    const quoted = [
      `**${escapeMarkdown(copyPlainText(props.heading, data))}**`,
      ...paragraphs.map((p) => copyMarkdown(p, data)),
      `*${escapeMarkdown(noteAttribution(noteSources(paragraphs, data)))}*`,
    ];
    return quoted.map((line) => `> ${line}`).join("\n>\n");
  },
});
