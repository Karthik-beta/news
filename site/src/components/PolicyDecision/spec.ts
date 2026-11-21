import { z } from "zod/v4";
import { formatValue, MISSING_TEXT } from "../../renderer/data/resolve";
import { escapeMarkdown } from "../../renderer/markdown";
import { defineNewsComponent } from "../contract";
import stepLineChart from "../StepLineChart/spec";
import { decisionChange, decisionFacts, decisionSources, isStepLine, PolicyDecision } from "./PolicyDecision";

const valueRef = z.string().regex(/^(dp|dv)_[a-z0-9]+(_[a-z0-9]+)*$/);
const factRef = z.string().regex(/^(dp|dv|pd|q|cx)_[a-z0-9]+(_[a-z0-9]+)*$/);
const lowerFirst = (s: string) => s.charAt(0).toLowerCase() + s.slice(1);

export default defineNewsComponent({
  kind: "component",
  name: "PolicyDecision",
  index: "PolicyDecision — rate decision — dp_ rate, dv_ change, facts[], path",
  description:
    "A central-bank or government decision: the new rate as a figure with its change in words, up to three supporting facts, and the rate path.",
  dataShape:
    'rate: the dp_… decided rate; change: a dp_… or dv_… change (written "Cut by …", "Raised by …" or "Unchanged" in its own unit); facts: up to three {label, value} with a dp_, dv_, pd_, q_ or cx_ value (vote, next meeting); path: a StepLineChart. Labels are text.',
  example: `decision = PolicyDecision("Federal funds rate, upper limit", "dp_fed_upper_20241218", "dp_fed_change_20241218", [{label: "Vote", value: "cx_fomc_vote_20241218"}, {label: "Next meeting", value: "pd_fomc_20250129"}], path)
path = StepLineChart("The Fed cut for the third meeting in a row", "Federal funds target range, upper limit, %", ["sr_fed_funds_upper"], ["pd_fomc_20240918", "pd_fomc_20241218"])`,
  props: z.object({
    label: z.string().min(1).describe("What the rate is, as text"),
    rate: valueRef.describe("dp_… the decided rate"),
    change: valueRef.nullable().optional().describe("dp_… or dv_… change, written in words"),
    facts: z
      .array(
        z.object({
          label: z.string().min(1).describe("What the fact is"),
          value: factRef.describe("dp_, dv_, pd_, q_ or cx_ reference"),
        }),
      )
      .max(3)
      .nullable()
      .optional()
      .describe("Up to three supporting facts"),
    path: stepLineChart.props.nullable().optional().describe("StepLineChart: the rate path"),
  }),
  component: PolicyDecision,
  markdown: ({ props, data, children }) => {
    const rate = data.value(props.rate);
    const change = decisionChange(data, props.change);
    const lines = [
      `- **${escapeMarkdown(data.plainText(props.label))}:** ${rate.ok ? escapeMarkdown(formatValue(rate)) : MISSING_TEXT}${change ? `, ${escapeMarkdown(lowerFirst(change))}` : ""}`,
      ...decisionFacts(props).map(
        (f) => `- **${escapeMarkdown(data.plainText(f.label))}:** ${escapeMarkdown(data.plainText(`{${f.value}}`))}`,
      ),
    ];
    const sources = decisionSources(data, props);
    const source = sources.length
      ? `\n\n${sources.length > 1 ? "Sources" : "Source"}: ${sources.map((s) => escapeMarkdown(`${s.source.publisher}, ${s.source.short}`)).join("; ")}.`
      : "";
    const path = isStepLine(props.path) ? `\n\n${children(props.path)}` : "";
    return `${lines.join("\n")}${source}${path}`;
  },
});
