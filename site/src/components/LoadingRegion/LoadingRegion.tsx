// The loading state (design/components.md, "Loading"): a dynamic list (search results) keeps its previous results
// at 50% opacity while new ones load. No skeleton and no spinner. The island that fetches sets `busy`; a polite
// status line, present from the first render, tells screen readers that results are loading. While busy the old
// results are inert: an inactive part of the interface, which is why their faded text is exempt from contrast.
import type { ReactNode } from "react";

export interface LoadingRegionProps {
  busy: boolean;
  /** What the status line says while busy. */
  loadingText?: string;
  children: ReactNode;
}

export function LoadingRegion({ busy, loadingText = "Loading results", children }: LoadingRegionProps) {
  return (
    <div className="loading-region">
      <span className="visually-hidden" role="status">
        {busy ? loadingText : ""}
      </span>
      <div className="loading-region__content" aria-busy={busy || undefined} inert={busy || undefined}>
        {children}
      </div>
    </div>
  );
}
