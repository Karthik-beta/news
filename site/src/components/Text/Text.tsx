import type { ElementType, ReactNode } from "react";

/**
 * The type styles of design/typography.md as one component. Each variant is a class in Text.css that uses the
 * type tokens only; components may use the classes directly (`text-body`) when a wrapper element is in the way.
 */
export const TEXT_VARIANTS = [
  "wordmark",
  "display",
  "headline-xl",
  "headline-l",
  "headline-m",
  "headline-s",
  "standfirst",
  "lead-summary",
  "body",
  "body-s",
  "fly",
  "chart-title",
  "chart-subtitle",
  "figure",
  "figure-s",
  "ui",
  "label",
  "source",
] as const;

export type TextVariant = (typeof TEXT_VARIANTS)[number];

export const textClass = (variant: TextVariant) => `text-${variant}`;

export interface TextProps {
  variant: TextVariant;
  /** The element to render. Defaults to `p`; headlines should pass their heading level. */
  as?: ElementType;
  /** Tabular, lining figures for anything that aligns vertically (chart values, table cells). */
  tabular?: boolean;
  className?: string;
  id?: string;
  children: ReactNode;
}

export function Text({ variant, as: Element = "p", tabular, className, id, children }: TextProps) {
  const classes = [textClass(variant), tabular ? "text-tabular" : "", className ?? ""].filter(Boolean).join(" ");
  return (
    <Element className={classes} id={id}>
      {children}
    </Element>
  );
}
