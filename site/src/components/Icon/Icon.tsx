// The eight icons of design/components.md ("Icons"): 1.6px strokes on a 16px grid, in the colour of the text
// around them. No icon fonts, no emoji, no decorative icons.
import type { ReactNode } from "react";

export const ICON_NAMES = ["search", "external", "download", "close", "play", "error", "check", "info"] as const;
export type IconName = (typeof ICON_NAMES)[number];

const PATHS: Record<IconName, ReactNode> = {
  search: (
    <>
      <circle cx="7" cy="7" r="4.6" />
      <path d="M10.4 10.4 14 14" />
    </>
  ),
  external: (
    <>
      <path d="M12.5 9v4.5h-10v-10H7" />
      <path d="M9.5 2.5h4v4M13.5 2.5 7.5 8.5" />
    </>
  ),
  download: (
    <>
      <path d="M8 2v8.2M4.6 6.8 8 10.2l3.4-3.4" />
      <path d="M2.5 10.5v3h11v-3" />
    </>
  ),
  close: <path d="M3.5 3.5l9 9M12.5 3.5l-9 9" />,
  play: <path className="icon__fill" d="M5 3v10l8-5z" />,
  error: (
    <>
      <circle cx="8" cy="8" r="6.2" />
      <path d="M8 4.6v4.4" />
      <circle className="icon__fill" cx="8" cy="11.4" r="0.95" />
    </>
  ),
  check: <path d="M2.8 8.6 6.2 12 13.2 4.6" />,
  info: (
    <>
      <circle cx="8" cy="8" r="6.2" />
      <path d="M8 7.2v4.4" />
      <circle className="icon__fill" cx="8" cy="4.8" r="0.95" />
    </>
  ),
};

/**
 * An icon sized to the surrounding text (1em). Decorative by default (hidden from assistive technology, since
 * the text beside it says what it means); pass `label` when the icon stands alone.
 */
export function Icon({ name, label, className }: { name: IconName; label?: string; className?: string }) {
  const a11y = label ? { role: "img", "aria-label": label } : { "aria-hidden": true as const };
  return (
    // biome-ignore lint/a11y/noSvgWithoutTitle: decorative icons are aria-hidden; labelled ones get role="img" and aria-label
    <svg
      className={`icon${className ? ` ${className}` : ""}`}
      viewBox="0 0 16 16"
      width="16"
      height="16"
      focusable="false"
      {...a11y}
    >
      {PATHS[name]}
    </svg>
  );
}
