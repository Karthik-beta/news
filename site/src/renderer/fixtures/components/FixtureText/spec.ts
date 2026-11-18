import { z } from "zod/v4";
import { defineNewsComponent } from "../../../../components/contract";
import { escapeMarkdown } from "../../../markdown";
import { FixtureText } from "./FixtureText";

export default defineNewsComponent({
  kind: "component",
  name: "FixtureText",
  index: "FixtureText — fixture only: paragraph — text with {ref} tokens",
  description: "Fixture-only paragraph for renderer tests; numbers only as {ref} tokens.",
  dataShape: "text: prose with {dp_…}, {dv_…}, {pd_…}, {cx_…} or {q_…} tokens and no digits.",
  example: `intro = FixtureText("Net sales rose to {dp_aapl_net_sales_fy2024} in {pd_fy2024}.")`,
  props: z.object({ text: z.string().describe("Prose; numbers only as {ref} tokens") }),
  component: FixtureText,
  markdown: ({ props, data }) => escapeMarkdown(data.plainText(props.text)),
});
