// An official video (design/components.md, "Video"). With a YouTube id it is click-to-load (the island in
// VideoEmbed.client.tsx): no request to YouTube before the reader's click, then only youtube-nocookie.com. The
// title, publisher and date and the privacy line are the figure's caption, static text under the 16:9 box, so they
// are never clipped or covered. A video that can't be embedded is a plain link line to the publisher's page
// ("Watch … on the … site"), with no island and no JavaScript.
import type { ComponentRenderer } from "@openuidev/react-lang";
import { formatDate } from "../../format";
import { Island } from "../../renderer/islands/Island";
import { Icon } from "../Icon/Icon";
import VideoEmbedIsland, { NOTICE } from "./VideoEmbed.client";

export interface VideoEmbedProps {
  title: string;
  publisher: string;
  url: string;
  youtubeId?: string | null;
  publishedOn?: string | null;
}

/** "Federal Reserve, 18 September 2024" */
export const videoMeta = (props: Pick<VideoEmbedProps, "publisher" | "publishedOn">) =>
  [props.publisher.trim(), props.publishedOn ? formatDate(props.publishedOn) : null].filter(Boolean).join(", ");

/** The link line for a video that can't be embedded. */
export const watchText = (props: Pick<VideoEmbedProps, "title" | "publisher">) =>
  `Watch “${props.title.trim()}” on the ${props.publisher.trim()} site`;

export const VideoEmbed: ComponentRenderer<VideoEmbedProps> = ({ props }) => {
  if (!props.youtubeId) {
    return (
      <p className="video-embed video-embed--link text-ui">
        <a href={props.url}>
          {watchText(props)} <Icon name="external" />
        </a>
      </p>
    );
  }
  return (
    <figure className="video-embed" aria-label={`Video: ${props.title.trim()}`}>
      <Island
        name="VideoEmbed"
        component={VideoEmbedIsland}
        props={{ youtubeId: props.youtubeId, title: props.title.trim(), url: props.url }}
      />
      <figcaption className="video-embed__caption">
        <span className="video-embed__title text-headline-s">{props.title.trim()}</span>
        <span className="video-embed__meta text-label">{videoMeta(props)}</span>
        <span className="video-embed__notice text-source">{NOTICE}</span>
      </figcaption>
    </figure>
  );
};
