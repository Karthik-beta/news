import { z } from "zod/v4";
import { escapeMarkdown } from "../../renderer/markdown";
import { defineNewsComponent } from "../contract";
import { VideoEmbed, videoMeta, watchText } from "./VideoEmbed";

export default defineNewsComponent({
  kind: "component",
  name: "VideoEmbed",
  index: "VideoEmbed — official video, click-to-load — embed record fields",
  description:
    "An official video: click-to-load from youtube-nocookie.com when it has a YouTube id; otherwise a link line to the publisher's page.",
  dataShape:
    "Copy title, publisher, url, youtubeId and publishedOn (YYYY-MM-DD) from the brief's embed record; leave youtubeId null when it can't be embedded.",
  example: `video = VideoEmbed("Press conference on the policy decision", "Federal Reserve", "https://www.federalreserve.gov/monetarypolicy/fomcpresconf20240918.htm", "pqHJyLAo3ac", "2024-09-18")`,
  props: z.object({
    title: z.string().min(1).describe("The video's own title"),
    publisher: z.string().min(1).describe("Who published it"),
    url: z.url({ protocol: /^https$/ }).describe("The video on its publisher's page (https)"),
    youtubeId: z
      .string()
      .regex(/^[A-Za-z0-9_-]{11}$/)
      .nullable()
      .optional()
      .describe("YouTube video id; null when it can't be embedded"),
    publishedOn: z
      .string()
      .regex(/^\d{4}-\d{2}-\d{2}$/)
      .nullable()
      .optional()
      .describe("YYYY-MM-DD from the embed record"),
  }),
  component: VideoEmbed,
  markdown: ({ props }) =>
    props.youtubeId
      ? `Video: [${escapeMarkdown(props.title.trim())}](${props.url}). ${escapeMarkdown(videoMeta(props))}.`
      : `[${escapeMarkdown(watchText(props))}](${props.url})`,
});
