import { type ElementNode, Renderer, useRenderNode } from "@openuidev/react-lang";
import { spikeLibrary } from "./library";

// Approach A helper: one static OpenUI node, rendered by OpenUI's runtime inside Astro's React renderer
// (no client directive, so Astro ships no JavaScript for it).
function NodeSlot({ node }: { node: ElementNode }) {
  return <>{useRenderNode()(node)}</>;
}

export function OuiStatic({ oui, node }: { oui: string; node: ElementNode }) {
  return (
    <Renderer.Root response={oui} library={spikeLibrary}>
      <NodeSlot node={node} />
    </Renderer.Root>
  );
}
