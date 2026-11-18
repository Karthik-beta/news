// The page's data, available to every component during a render. Plain React context: it works the same under
// OpenUI's Renderer.Root (approach B) and the fallback walker (C), and needs no OpenUI runtime hooks.
import { createContext, type ReactNode, useContext } from "react";
import type { PageDataResolver } from "./resolve";

const PageDataContext = createContext<PageDataResolver | null>(null);

export function PageDataProvider({ resolver, children }: { resolver: PageDataResolver; children: ReactNode }) {
  return <PageDataContext.Provider value={resolver}>{children}</PageDataContext.Provider>;
}

/** The resolver for the page being rendered. Components call this to turn references into values. */
export function usePageData(): PageDataResolver {
  const resolver = useContext(PageDataContext);
  if (!resolver) {
    throw new Error("usePageData() needs a <PageDataProvider>: render pages through site/src/renderer.");
  }
  return resolver;
}
