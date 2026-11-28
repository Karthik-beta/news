import { z } from "zod/v4";
import { escapeMarkdown } from "../../renderer/markdown";
import { defineNewsComponent } from "../contract";
import { copyMarkdown } from "../Prose/copy";
import { Steps } from "./Steps";

export default defineNewsComponent({
  kind: "component",
  name: "Steps",
  index: "Steps — how something works, in order — {title, text}[] 2-8",
  description:
    "An explainer's numbered steps: how a process works, in order, each a short title and one or two sentences with {ref} tokens.",
  dataShape:
    "steps: 2–8 {title, text} pairs in order; title is a few words, text has {ref} tokens for every value, period and quote. Numbers come from the order.",
  example: `how = Steps([{title: "The committee meets", text: "Policymakers meet eight times a year; the last meeting ended on {pd_fomc_20241218}."}, {title: "It sets a range", text: "The upper limit is now {dp_fed_upper_20241218}."}])`,
  props: z.object({
    steps: z
      .array(
        z.object({
          title: z.string().min(1).max(60).describe("A few words naming the step"),
          text: z.string().min(1).describe("What happens, with {ref} tokens"),
        }),
      )
      .min(2)
      .max(8)
      .describe("Steps in order"),
  }),
  component: Steps,
  markdown: ({ props, data }) =>
    props.steps
      .map((s, i) => `${i + 1}. **${escapeMarkdown(data.plainText(s.title))}.** ${copyMarkdown(s.text, data)}`)
      .join("\n"),
});
