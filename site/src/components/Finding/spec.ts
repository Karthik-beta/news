import { z } from "zod/v4";
import { formatValue, MISSING_TEXT } from "../../renderer/data/resolve";
import { escapeMarkdown } from "../../renderer/markdown";
import { BADGES } from "../Badge/Badge";
import { defineNewsComponent } from "../contract";
import { copyMarkdown } from "../Prose/copy";
import { Finding, figureLabel, findingBadges, findingSources } from "./Finding";

const valueRef = z.string().regex(/^(dp|dv)_[a-z0-9]+(_[a-z0-9]+)*$/);

export default defineNewsComponent({
  kind: "component",
  name: "Finding",
  index: "Finding — one science finding, method, badge — text, dp_ figure",
  description:
    "One finding from a study or agency release, stated in one sentence, with its key number, the method in one line and the source type as a badge.",
  dataShape:
    "text: one sentence with {ref} tokens; figure: the dp_… or dv_… key number; label: what the figure is (else the data point's label); method: one line with {ref} tokens. The badge comes from sources.json.",
  example: `walking = Finding("Walking daily lowered blood pressure in a randomised trial of {dp_trial_participants} adults", "dv_trial_bp_change", "Change in systolic blood pressure", "Peer-reviewed paper, {dp_trial_participants} participants, randomised controlled trial")`,
  props: z.object({
    text: z.string().min(1).describe("The finding in one sentence"),
    figure: valueRef.nullable().optional().describe("dp_… or dv_… key number"),
    label: z.string().nullable().optional().describe("What the figure is"),
    method: z.string().nullable().optional().describe("The method in one line"),
  }),
  component: Finding,
  markdown: ({ props, data }) => {
    const lines = [`**${copyMarkdown(props.text, data)}**`];
    if (props.figure) {
      const v = data.value(props.figure);
      const label = figureLabel(data, props);
      lines.push(`${label ? `${escapeMarkdown(label)}: ` : ""}${v.ok ? escapeMarkdown(formatValue(v)) : MISSING_TEXT}`);
    }
    if (props.method) lines.push(copyMarkdown(props.method, data));
    const sources = findingSources(data, props);
    const badges = findingBadges(sources).map((b) => BADGES[b]);
    if (badges.length) lines.push(`Source type: ${badges.join(", ")}`);
    if (sources.length) {
      lines.push(
        `${sources.length > 1 ? "Sources" : "Source"}: ${sources.map((s) => escapeMarkdown(`${s.source.publisher}, ${s.source.short}`)).join("; ")}.`,
      );
    }
    return lines.join("\n\n");
  },
});
