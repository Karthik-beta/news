import { z } from "zod/v4";
import { SOURCE_ID_PATTERN } from "../../renderer/bundle/schema";
import { MISSING_TEXT } from "../../renderer/data/resolve";
import { escapeMarkdown } from "../../renderer/markdown";
import { BADGES } from "../Badge/Badge";
import { defineNewsComponent } from "../contract";
import { sourceBadges } from "../SourcesPanel/SourcesPanel";
import { SourceCard, sourceCardMeta } from "./SourceCard";

export default defineNewsComponent({
  kind: "component",
  name: "SourceCard",
  index: "SourceCard — the primary document, linked — src_ ref",
  description:
    "The one document the page rests on (a report, paper or filing), linked, with its badges, type, publisher and date from sources.json.",
  dataShape: "source: a src_… ID from sources.json; every word shown comes from that source record.",
  example: `report = SourceCard("src_imf_weo_oct_2024")`,
  props: z.object({
    source: z.string().regex(SOURCE_ID_PATTERN).describe("src_… the document"),
  }),
  component: SourceCard,
  markdown: ({ props, data }) => {
    const cited = data.citeSource(props.source);
    if (!cited) return MISSING_TEXT;
    const { source } = cited;
    const badges = sourceBadges(source).map((k) => BADGES[k]);
    return `**[${escapeMarkdown(source.title)}](${source.url})**\n\n${escapeMarkdown([...badges, sourceCardMeta(source)].join(". "))}.`;
  },
});
