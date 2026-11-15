// Client side of the islands: the only JavaScript a page ships, and only when it has an island. Each island is
// hydrated when it first comes near the viewport (200px margin), with its component loaded on demand, so a page
// downloads only the islands it shows.
import { createElement } from "react";
import { hydrateRoot } from "react-dom/client";
import type { IslandLoaders } from "./types";

async function hydrate(el: HTMLElement, loaders: IslandLoaders): Promise<void> {
  const name = el.dataset.island ?? "";
  const load = loaders[name];
  if (!load) {
    console.error(`No island named "${name}" in the registry`);
    return;
  }
  const { default: component } = await load();
  hydrateRoot(el, createElement(component, JSON.parse(el.dataset.props ?? "{}")), {
    identifierPrefix: el.dataset.islandPrefix,
    onRecoverableError: (error) => console.error("island hydration error", name, error),
  });
}

export function hydrateIslands(loaders: IslandLoaders, root: ParentNode = document): void {
  const els = [...root.querySelectorAll<HTMLElement>("[data-island]")];
  const start = (el: HTMLElement) => {
    hydrate(el, loaders).catch((error) => console.error("island failed to load", el.dataset.island, error));
  };
  if (!("IntersectionObserver" in window)) {
    els.forEach(start);
    return;
  }
  const io = new IntersectionObserver(
    (entries) => {
      for (const entry of entries) {
        if (!entry.isIntersecting) continue;
        io.unobserve(entry.target);
        start(entry.target as HTMLElement);
      }
    },
    { rootMargin: "200px" },
  );
  for (const el of els) io.observe(el);
}
