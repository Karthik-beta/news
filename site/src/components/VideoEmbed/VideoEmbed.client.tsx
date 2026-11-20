// The video's interactive part (an island): click-to-load (design/components.md, "Video"). Nothing is requested
// from YouTube until the reader asks: before that the page shows a wash 16:9 box with a play button in its centre.
// The title, publisher and date and the privacy line are the figure's caption under the box (VideoEmbed.tsx), so
// no width, zoom or text size can clip them or put the button over them. The click swaps in the
// youtube-nocookie.com player at the same size and moves focus into it.
//
// Server markup and the first client render match: the play control is a link to the video on its own site, so it
// works with JavaScript off. Once hydrated it becomes a button that loads the player in place.
import { useEffect, useRef, useState } from "react";
import { Icon } from "../Icon/Icon";

export interface VideoEmbedIslandProps {
  /** YouTube video id (11 characters). */
  youtubeId: string;
  title: string;
  /** The video on its publisher's site or on YouTube: where the link goes without JavaScript. */
  url: string;
}

export const NOTICE = "Loads from YouTube. YouTube will receive your IP address.";
export const embedUrl = (id: string) => `https://www.youtube-nocookie.com/embed/${encodeURIComponent(id)}?autoplay=1`;

export default function VideoEmbedIsland({ youtubeId, title, url }: VideoEmbedIslandProps) {
  const [hydrated, setHydrated] = useState(false);
  const [loaded, setLoaded] = useState(false);
  const player = useRef<HTMLIFrameElement>(null);

  useEffect(() => setHydrated(true), []);
  useEffect(() => {
    if (loaded) player.current?.focus();
  }, [loaded]);

  const label = `Load video from YouTube: ${title}`;
  const play = <Icon name="play" className="video-embed__play-icon" />;

  return (
    <div className="video-embed__frame">
      {loaded ? (
        <iframe
          ref={player}
          className="video-embed__player"
          src={embedUrl(youtubeId)}
          title={title}
          allow="autoplay; encrypted-media; picture-in-picture; fullscreen"
          allowFullScreen
          referrerPolicy="strict-origin-when-cross-origin"
        />
      ) : hydrated ? (
        <button type="button" className="video-embed__play" aria-label={label} onClick={() => setLoaded(true)}>
          {play}
        </button>
      ) : (
        <a className="video-embed__play" href={url} aria-label={label}>
          {play}
        </a>
      )}
    </div>
  );
}
