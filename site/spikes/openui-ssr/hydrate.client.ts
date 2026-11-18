import { createElement } from "react";
import { hydrateRoot } from "react-dom/client";
import { type IslandName, islands } from "./islands";

// Client side of the islands: the only JavaScript a page ships, and only when it has an island.
// Hydrates each island when it first comes near the viewport (like Astro's client:visible).
function hydrate(el: HTMLElement) {
  const name = el.dataset.island as IslandName;
  const component = islands[name];
  if (!component) return;
  hydrateRoot(el, createElement(component, JSON.parse(el.dataset.props ?? "{}")), {
    onRecoverableError: (error) => console.error("island hydration error", name, error),
  });
}

const els = [...document.querySelectorAll<HTMLElement>("[data-island]")];
if ("IntersectionObserver" in window) {
  const io = new IntersectionObserver(
    (entries) => {
      for (const entry of entries) {
        if (!entry.isIntersecting) continue;
        io.unobserve(entry.target);
        hydrate(entry.target as HTMLElement);
      }
    },
    { rootMargin: "200px" },
  );
  for (const el of els) io.observe(el);
} else {
  for (const el of els) hydrate(el);
}
