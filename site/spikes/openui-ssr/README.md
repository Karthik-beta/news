# Spike: server rendering of OpenUI

Proof and measurements behind [ADR 0001](../../../docs/adr/0001-openui-server-rendering.md). This is spike code, kept only as the measurement record: the production renderer is `site/src/renderer/` (with its own unit, Bun SSR and browser tests) and the components are in `site/src/components/`. Nothing here is built or tested by default; set `OPENUI_SPIKE=1` to re-run the measurements below.

## What is here

| File | Role |
|---|---|
| `fixture.oui` | The fixture page in OpenUI Lang: headline, paragraph, key figure, line chart |
| `library.tsx` | Four-component OpenUI library (`defineComponent`, `createLibrary`) |
| `LineChart.tsx` | The interactive chart (the island); marks itself `data-hydrated="true"` once live |
| `islands.ts`, `Island.tsx`, `hydrate.client.ts` | Island registry, the server wrapper (`data-island` + `data-props` + the island's own `renderToString`), and the client hydrator |
| `render.tsx` | Approach B (`Renderer.Root` + `renderToStaticMarkup`) and approach C, the fallback (own walker over the parse tree) |
| `AstroIslandsPage.astro`, `OuiStatic.tsx` | Approach A (Astro page, native `client:visible` island) |
| `ReactSsrPage.astro`, `FixtureLayout.astro`, `styles.css` | Hosts approach B's HTML in the Astro build so the browser test can load it |
| `whole-page.client.tsx` | Only sized, never served: the cost of hydrating the whole page instead of islands |
| `measure-*.ts`, `stats.ts`, `vitest.config.ts` | Measurement scripts; results merge into `results.json` |

Routes: `site/src/pages/_fixtures/openui-ssr/[approach].astro`, added by the `news-fixture-pages` integration (`site/astro.config.mjs`) only with `OPENUI_SPIKE=1`. It builds `/fixtures/openui-ssr/react-ssr/`, and `/fixtures/openui-ssr/astro-islands/` only with `OPENUI_SPIKE_A=1` as well. Approach A writes Astro's island `<style>` inside `<article>`, which fails the HTML validity check. Browser test: `site/tests/e2e/spikes/openui-ssr.spec.ts`.

## Re-running the measurements

From `site/`, one at a time (each is CPU-heavy on the laptop). In PowerShell, set variables with `$env:NAME = '1'` instead.

```sh
export OPENUI_TELEMETRY_DISABLED=1

# 1. Build with both fixture routes, then size HTML and every JS file each built page loads
OPENUI_SPIKE=1 OPENUI_SPIKE_A=1 bun --bun astro build
bun --bun spikes/openui-ssr/measure-dist.ts

# 2. B and C in Bun: p50/p95 over 50 renders, HTML size, island JS via Bun.build, whole-page hydration JS
bun --bun spikes/openui-ssr/measure-react.ts

# 3. A (and B hosted in Astro) through the Astro Container API, under bun --bun
bun --bun vitest run --config spikes/openui-ssr/vitest.config.ts

# 4. Browser check in Chromium: static HTML, static parts never hydrate, island hydrates, no hydration warnings
E2E_SKIP_BUILD=1 OPENUI_SPIKE=1 OPENUI_SPIKE_A=1 bunx playwright test tests/e2e/spikes --project='smoke-*'
```

`results.json` keeps the latest numbers: `dist_*` from step 1, `B_reactServer` / `C_ownWalker` / `wholePageHydrationJs` from step 2, and `A_astroContainer` / `B_inAstroContainer` from step 3. Timings vary by about 2× at p95 when other work is running. Compare options within one run, not across runs. `firstMs` is the first call in the process, so only the first approach measured in a script is truly cold.

Without `OPENUI_SPIKE_A=1`, step 1 measures only B, and step 4 tests only B. Without `OPENUI_SPIKE=1` nothing here is built and the spec has no tests, which is what CI runs; the production renderer's browser test is `site/tests/e2e/renderer/fixture-pages.spec.ts`.
