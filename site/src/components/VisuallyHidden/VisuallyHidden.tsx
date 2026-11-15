import type { ElementType, ReactNode } from "react";

/**
 * Text for screen readers only: present in the accessibility tree, invisible on screen. Use it to complete a
 * sentence the layout implies ("Source:" before a cited figure), never to hide content sighted readers need.
 * `focusable` shows it again while it or something inside has focus (skip links).
 */
export function VisuallyHidden({
  as: Element = "span",
  focusable = false,
  id,
  children,
}: {
  as?: ElementType;
  focusable?: boolean;
  id?: string;
  children: ReactNode;
}) {
  return (
    <Element className={focusable ? "visually-hidden visually-hidden--focusable" : "visually-hidden"} id={id}>
      {children}
    </Element>
  );
}
