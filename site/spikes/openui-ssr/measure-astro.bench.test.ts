// Measures approach A: the fixture page rendered by the Astro Container API (with the React renderer),
// p50/p95 over 50 renders, plus the HTML size. Runs in Vitest because .astro files need Astro's compiler.
// Run from site/: bun --bun vitest run --config spikes/openui-ssr/vitest.config.ts

import { loadRenderers } from "astro:container";
import { fileURLToPath } from "node:url";
import { getContainerRenderer } from "@astrojs/react/container-renderer";
import { experimental_AstroContainer as AstroContainer } from "astro/container";
import { expect, it } from "vitest";
import AstroIslandsPage from "./AstroIslandsPage.astro";
import ReactSsrPage from "./ReactSsrPage.astro";
import { bench, sizes, writeResults } from "./stats";

const results = fileURLToPath(new URL("./results.json", import.meta.url));

it("renders the fixture page with the Container API, 50 times", { timeout: 120_000 }, async () => {
  const container = await AstroContainer.create({ renderers: await loadRenderers([getContainerRenderer()]) });
  const renderA = () => container.renderToString(AstroIslandsPage, { partial: false });
  const renderBInAstro = () => container.renderToString(ReactSsrPage, { partial: false });

  const a = await bench(renderA);
  const bInAstro = await bench(renderBInAstro);
  const htmlA = await renderA();
  const htmlB = await renderBInAstro();

  expect(htmlA).toContain("<astro-island");
  expect(htmlB).toContain('data-island="LineChart"');
  writeResults(results, {
    A_astroContainer: {
      render: a,
      html: sizes(htmlA),
      runtime: `vitest under ${process.versions.bun ? `bun ${process.versions.bun}` : `node ${process.version}`}`,
    },
    B_inAstroContainer: { render: bInAstro, html: sizes(htmlB) },
  });
});
