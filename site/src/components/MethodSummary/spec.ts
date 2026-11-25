import { z } from "zod/v4";
import { escapeMarkdown } from "../../renderer/markdown";
import { defineNewsComponent } from "../contract";
import { copyMarkdown } from "../Prose/copy";
import { MethodSummary, methodRows } from "./MethodSummary";

const row = (what: string) => z.string().nullable().optional().describe(what);

export default defineNewsComponent({
  kind: "component",
  name: "MethodSummary",
  index: "MethodSummary — how a study was done — design, sample, period…",
  description:
    "How a study or major report was done: source type, study design, sample, period and the limitations the authors state; leave out what the source does not say.",
  dataShape:
    "Each value is text with {ref} tokens for numbers (sample size {dp_…}, period {pd_…}); limitations preferably as the authors' words, {q_…}.",
  example: `method = MethodSummary("Peer-reviewed paper", "Randomised controlled trial", "{dp_trial_participants} adults", "{pd_trial_period}", "{q_trial_limitations}")`,
  props: z.object({
    sourceType: row("Peer-reviewed paper, preprint, dataset, report"),
    design: row("Study design"),
    sample: row("Who or what was studied; numbers as {ref} tokens"),
    period: row("When; a {pd_…} token"),
    limitations: row("As the authors state them; a {q_…} token where possible"),
  }),
  component: MethodSummary,
  markdown: ({ props, data }) =>
    methodRows(props)
      .map((r) => `- **${escapeMarkdown(r.label)}:** ${copyMarkdown(r.value, data)}`)
      .join("\n"),
});
