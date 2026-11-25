import { z } from "zod/v4";
import { refPattern } from "../../renderer/bundle/schema";
import { MISSING_TEXT } from "../../renderer/data/resolve";
import { escapeMarkdown } from "../../renderer/markdown";
import { defineNewsComponent } from "../contract";
import { copyMarkdown } from "../Prose/copy";
import { Timeline } from "./Timeline";

const periodRef = z.string().regex(refPattern("pd"));

export default defineNewsComponent({
  kind: "component",
  name: "Timeline",
  index: "Timeline — dated events, vertical — {date: pd_, text}[], pd_ key",
  description:
    "A vertical list of dated events, oldest first, each with its date and one or two sentences; one key event can be marked.",
  dataShape:
    "events: {date, text} pairs, where date is a pd_… period and text has {ref} tokens for values and quotes; highlight: the pd_… of the key event.",
  example: `steps = Timeline([{date: "pd_fomc_20240918", text: "The Fed cut its rate by {dp_fed_change_20240918}."}, {date: "pd_fomc_20241218", text: "A third cut, to {dp_fed_upper_20241218}."}], "pd_fomc_20240918")`,
  props: z.object({
    events: z
      .array(
        z.object({
          date: periodRef.describe("pd_… the event's date"),
          text: z.string().min(1).describe("What happened, with {ref} tokens"),
        }),
      )
      .min(1)
      .max(40)
      .describe("Events in time order"),
    highlight: periodRef.nullable().optional().describe("pd_… of the key event"),
  }),
  component: Timeline,
  markdown: ({ props, data }) =>
    props.events
      .map((e, i, all) => {
        const date = data.period(e.date)?.label ?? MISSING_TEXT;
        const keyIndex = props.highlight == null ? -1 : all.findIndex((x) => x.date === props.highlight);
        const key = i === keyIndex ? " (key event)" : "";
        return `- **${escapeMarkdown(date)}${key}:** ${copyMarkdown(e.text, data)}`;
      })
      .join("\n"),
});
