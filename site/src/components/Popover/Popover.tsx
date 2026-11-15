// The shared popover and tooltip base (design/components.md: "Citation popover", "Tooltip"). One look for both:
// paper, a 1px rule border, no shadow, 12px padding, at most 320px wide.
//
// Popover: zero JavaScript. It uses the browser's own popover attribute: the trigger button opens and closes it
// (click, tap, Enter, Space), Escape and tapping outside close it, and focus returns to the trigger. Where the
// browser supports anchor positioning it sits under its trigger; elsewhere it shows where the browser puts it.
// Tooltip: the same look for islands that position it themselves (chart tooltips follow the pointer or the arrow
// keys), so it takes computed geometry as inline style, the one inline style the design rules allow.
import type { CSSProperties, ReactNode } from "react";

export interface PopoverProps {
  /** Unique on the page; links the trigger and the panel. */
  id: string;
  /** The trigger's content (a cited figure, a glossary term). Rendered inside a <button>. */
  trigger: ReactNode;
  /** Extra classes for the trigger (the cited figure's style). */
  triggerClassName?: string;
  /** Accessible name for the panel ("Source for $391.0bn"). */
  label: string;
  children: ReactNode;
}

/**
 * Trigger and panel, both phrasing content, so a popover can sit inside a sentence, a heading or a key figure.
 * The panel's content must be phrasing content too (spans with classes, links, strong).
 */
export function Popover({ id, trigger, triggerClassName, label, children }: PopoverProps) {
  return (
    <>
      <button
        type="button"
        className={`popover-trigger${triggerClassName ? ` ${triggerClassName}` : ""}`}
        popoverTarget={id}
      >
        {trigger}
      </button>
      <span id={id} popover="auto" className="popover" role="dialog" aria-label={label}>
        {children}
      </span>
    </>
  );
}

export interface TooltipProps {
  id?: string;
  /** Position inside the island's positioned container, computed by the island: px, or any CSS length ("40%"). */
  x: number | string;
  y: number | string;
  /** Which side of the point the tooltip sits on; the island flips it near the edges. */
  placement?: "above" | "below";
  align?: "start" | "center" | "end";
  children: ReactNode;
}

/**
 * A tooltip for hydrated islands. The island decides when it shows (hover, focus, arrow keys, tap) and where.
 * Announced politely, so keyboard and screen-reader users hear the value the tooltip shows.
 */
export function Tooltip({ id, x, y, placement = "above", align = "center", children }: TooltipProps) {
  const length = (v: number | string) => (typeof v === "number" ? `${v}px` : v);
  const style: CSSProperties = { left: length(x), top: length(y) };
  return (
    <span
      id={id}
      className={`popover popover--tooltip popover--${placement} popover--align-${align}`}
      role="status"
      style={style}
    >
      {children}
    </span>
  );
}
