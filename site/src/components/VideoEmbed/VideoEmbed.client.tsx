// The video's interactive part (an island): click-to-load (design/components.md, "Video"). Nothing is requested
// from YouTube until the reader asks: before that the page shows a wash 16:9 box with the title, publisher and date,
// a play button and the line saying YouTube will receive their IP address. The click swaps in the
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
  /** "Federal Reserve, 18 September 2024" */
  meta: string;
  /** The video on its publisher's site or on YouTube: where the link goes without JavaScript. */
  url: string;
}

export const NOTICE = "Loads from YouTube. YouTube will receive your IP address.";
export const embedUrl = (id: string) => `https://www.youtube-nocookie.com/embed/${encodeURIComponent(id)}?autoplay=1`;

export default function VideoEmbedIsland({ youtubeId, title, meta, url }: VideoEmbedIslandProps) {
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
      ) : (
        <div className="video-embed__placeholder">
          <p className="video-embed__title text-headline-s">{title}</p>
          <p className="video-embed__meta text-label">{meta}</p>
          {hydrated ? (
            <button type="button" className="video-embed__play" aria-label={label} onClick={() => setLoaded(true)}>
              {play}
            </button>
          ) : (
            <a className="video-embed__play" href={url} aria-label={label}>
              {play}
            </a>
          )}
          <p className="video-embed__notice text-source">{NOTICE}</p>
        </div>
      )}
    </div>
  );
}
