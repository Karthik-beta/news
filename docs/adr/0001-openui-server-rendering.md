# ADR 0001: Server rendering of OpenUI pages

- Status: accepted
- Date: 2026-11-15
- Phase: Development Plan Phase 1 (Spike: server rendering of OpenUI); issue #4
- Spike code and re-run instructions: `site/spikes/openui-ssr/` (README there); raw numbers in `site/spikes/openui-ssr/results.json`

## Context

Pages are written in OpenUI Lang (`page.oui`) and must reach readers as static HTML from the CDN. Only interactive parts, such as chart tooltips, may load JavaScript. The OpenUI docs don't cover server rendering (Technical Design, "Risks"), so this was the riskiest assumption in the plan. The spike had to answer three questions:

1. Does an OpenUI Lang page render to complete static HTML under `bun --bun`, both through Astro's Container API and through React server rendering?
2. Can one chart hydrate as an island while the static parts ship 0 KB of JavaScript, with no hydration warnings?
3. If neither works, does our own server renderer over the OpenUI Lang syntax tree work instead?

Packages (pinned exactly): `@openuidev/react-lang` 1.1.1 (parser, `Renderer`, `createLibrary`, `defineComponent`; it depends on `@openuidev/lang-core` 1.1.1), `@openuidev/react-ui` 0.18.8, `@openuidev/react-headless` 0.18.8, `zod` 4.6.7, `zustand` 4.5.7. React 19.3, Astro 7.4, Bun 1.4.3.

Fixture: `site/spikes/openui-ssr/fixture.oui`. It has a headline, a paragraph, a key figure and a five-point line chart, written against a four-component spike library (`library.tsx`). The chart is the only interactive part: its tooltip follows the hover point or the arrow keys.

## Options

**A. Astro page with a native Astro island.** An `.astro` page parses the OpenUI Lang. Static nodes are rendered by OpenUI's React runtime through `@astrojs/react` with no client directive. The chart is placed as `<LineChart client:visible>`. Timings come from the Container API (`experimental_AstroContainer` with the React renderer) inside Vitest under `bun --bun`.

**B. React server rendering (`react-dom/server`) of OpenUI's runtime, with our own islands.** `renderToStaticMarkup(<Renderer.Root response library><RootSlot/></Renderer.Root>)` renders the page. `RootSlot` renders the parsed root with `useRenderNode()`. Interactive components render an `<Island>` wrapper: a `<div data-island="LineChart" data-props="…">` holding the component's own `renderToString` output, rendered in a separate React root so its markup matches what `hydrateRoot` expects, wherever the chart sits in the tree. A small client entry (`hydrate.client.ts`) hydrates each `[data-island]` when it nears the viewport.

**C. Fallback: our own renderer over the syntax tree.** Parse with `createParser(library.toJSONSchema(), library.root)`, walk the `ElementNode` tree, and map each `typeName` to a React component. OpenUI's React runtime is not involved. Islands work as in B.

## Measurements

Windows 11 laptop (4 cores), Bun 1.4.3, 15 November 2026. Each render time is p50 and p95 over 50 renders after 5 warm-up renders, parse included. "First" is the first (cold) call in a fresh process. HTML and JS sizes are raw / gzip (level 9).

| | A: Astro Container API, Astro island | **B: React server rendering (chosen)** | C: own walker (fallback) |
|---|---|---|---|
| Renders to complete static HTML under `bun --bun` | Yes | Yes | Yes (byte-identical to B) |
| Render time p50 / p95 | 8.9 ms / 14.7 ms | 4.0 ms / 12.7 ms bare in Bun; 3.3 ms / 6.8 ms when the same page goes through the Astro container | 1.2 ms / 2.1 ms |
| First (cold) render | 86 ms | 126 ms | not measured cold (ran after B) |
| Rendered document HTML | 8.18 / 3.28 KB | 2.47 / 1.00 KB | 2.47 / 1.00 KB |
| Built page in `dist/` (with the site head) | 9.94 / 3.78 KB | 4.51 / 1.68 KB | – |
| JS shipped, whole page | 223.0 / 69.7 KB (2.0 KB gzip of it inline island runtime) | 216.6 / 67.3 KB (Astro/Vite bundle); 209.6 / 65.5 KB (`Bun.build`) | same as B |
| JS for the static parts | 0 KB | 0 KB | 0 KB |
| Island hydrates, no hydration warnings (Playwright, Chromium) | Yes | Yes | same client as B |
| Valid HTML (`html-validate`) | **No**: Astro injects its island `<style>` and `<script>` inside `<article>` at the first island | Yes | Yes |
| Islands at any depth of the OpenUI tree | **No**: Astro islands can only be placed from `.astro`, so only top-level nodes can be islands without our own tree walk | Yes | Yes |

Repeat runs varied by about 2× at p95 (an earlier B run: p50 3.9 ms, p95 6.4 ms) because the laptop also runs other agents. The order between the options did not change. A's timings come from Vite's development transform inside Vitest, which flatters nobody. B inside the same container (3.3 ms) shows the gap is A's design (several `Renderer.Root`s and Astro's island machinery), not the measuring harness.

Almost all of the shipped JS is the React DOM client (about 60 KB gzip). The chart island itself is about 1 KB. The JS is the same file on every page, so readers download it once. For comparison, hydrating the whole page with OpenUI's runtime (parser, runtime and every component) would ship 605 / 186 KB.

Findings that shaped the decision:

- **`<Renderer>` server-renders fully.** It parses during render (`useMemo`), not in an effect, and its stores use `useSyncExternalStore` with a server snapshot. But the default `<Renderer>` wraps output in two `div`s with inline styles (`position:relative`, an opacity transition for query loading). That breaks the tokens-only, no-inline-style rule. `Renderer.Root` with our own root slot gives clean semantic markup.
- **Static parts are never hydrated.** The Playwright test checks that React attaches fibres only to the chart, not to the headline, paragraph or key figure. With JavaScript off, the page and the drawn chart are complete.
- **`renderToString` inside a server render works** for the island subtree. Rendering it as its own root keeps the island's markup (text separators, `useId` values) identical to what the client produces.

## Decision

**B.** `page.oui` is rendered to static HTML by `react-dom/server` (`renderToStaticMarkup`) in Bun, using OpenUI's own runtime: `Renderer.Root` plus a root slot, never the default `<Renderer>`. Interactive components are islands. They are rendered by an `Island` wrapper as `data-island` + JSON `data-props` + their own `renderToString` output, and hydrated by one shared, hashed client entry that `hydrateRoot`s each island when visible. A page with no islands gets no `<script>` at all.

The renderer (`site/src/renderer/`) and the warm render server (#17) call this directly. Astro keeps the site shell, hubs and fixture pages, and can host the same string (`set:html`) where a page needs to sit inside an Astro layout. Production rendering doesn't depend on the experimental Container API.

Why not A: slower (8.9 vs 4.0 ms p50), two to three times the HTML, invalid HTML from the injected island runtime, and islands only at the top level unless we write our own tree walk anyway. That walk is option C with Astro on top.

## Fallback

**C, already proven.** `renderWithOwnWalker` in `site/spikes/openui-ssr/render.tsx` parses with OpenUI's parser and maps nodes to components itself. On the fixture it produces byte-identical HTML to B and runs 3× faster. Switch to C if any of these happens:

- an OpenUI upgrade moves parsing or rendering into effects, so server output becomes empty;
- `Renderer.Root` starts emitting wrapper markup or inline styles we can't avoid;
- runtime cost matters at volume.

To keep the switch cheap, components are written only against the `ComponentRenderer` contract (`{ props, renderNode }`) and never call OpenUI hooks that need the runtime (state, forms, queries, actions) on static pages. Any of those would be an island. If OpenUI's parser itself breaks, pin the last good version. The OpenUI Lang grammar (statements, positional args, references) is small enough to parse ourselves as a last resort.

## Component index token budget

`component-index.json` is rendered into the cached prompt prefix as one line per component (name, purpose, data shape). **Budget: 4,000 tokens for the whole index**, and no single entry over 24 tokens.

Reasoning:

- The Technical Design budgets the whole cached prefix (instructions, brand rules, component index) at 5–8k tokens. OpenUI's syntax rules alone take about 2.3k characters (roughly 600 tokens; measured from `library.prompt()` for the spike library). With the page-writer instructions and brand rules at about 3–4k, an 8k ceiling leaves about 4k for the index.
- The library will have "hundreds" of components. At 250 components, 4,000 tokens is 16 tokens per line. That fits name + purpose + data shape if the purpose is a short phrase ("Line chart: change over time, 1–2 series; data: series[] of {period, value}").
- OpenUI's own signature format (`LineChart(title: string, subtitle: string, …) — description`) measured 48–129 characters per component here, about 25–35 tokens. At 250 components that's 7–9k tokens, more than the whole prefix. So full signatures stay out of the prefix and come from `news component <type>` on demand, as the design already says.
- If the library grows past what fits, group components by family (with `componentGroups`) and keep one line per family plus the most-used components. Don't raise the budget, because every page call pays for the prefix (cached reads still count).

Enforcement: the build test counts tokens offline and deterministically as `ceil(UTF-8 bytes / 3)`. That over-counts English and code for Claude, so passing it leaves headroom. The test fails over 4,000. The real cached-prefix size comes from `--output-format json` usage in ClickHouse telemetry. If the real index count goes over 4,000, or the estimator turns out to under-count, revisit this ADR.

## Consequences

- **Pin OpenUI exactly and upgrade on purpose.** `react-ui` shipped seven patch releases (0.18.2 to 0.18.8) between 5 and 12 November 2026. The Playwright test and an SSR unit test catch an upgrade that breaks server rendering.
- **Install telemetry.** `@openuidev/lang-core` has a `postinstall` that sends anonymous install telemetry. Bun blocks it because the package isn't in `trustedDependencies`; keep it that way. Runtime telemetry is opt-in only (`OPENUI_RUNTIME_TELEMETRY_ENABLED`). Set `OPENUI_TELEMETRY_DISABLED=1` (or `DO_NOT_TRACK=1`) in CI and in the render environment anyway.
- **Island contract.** Island props must be plain JSON (no functions, dates or class instances). Island components may not import server-only code. Server and client read the same registry (`islands.ts`), and only registered components ship JS. The first island on a page costs about 65 KB gzip of React DOM, cached across pages.
- **The OUI is parsed twice per render** in B (once to validate and get the root, once inside `Renderer.Root`). That's milliseconds, and keeps validation errors under our control.
- **`@openuidev/react-ui` components were not tested for server rendering** here. They use `goober` (runtime CSS-in-JS), Radix and their own styles. The component builder must check each adopted base component renders to complete HTML, ships no runtime CSS injection for static use, and follows the tokens-only rule. Otherwise we wrap or rebuild it.
- **Fixture routes.** Astro doesn't route folders that start with `_`, so the `src/pages/__fixtures/` convention in `site/TESTING.md` builds nothing. This spike uses `src/pages/fixtures/`, and TESTING.md should be updated to match. Fixture pages then ship in the static build (`noindex`), so it should be decided whether production builds exclude them.
- Approach A's fixture route is built only with `OPENUI_SPIKE_A=1`, so the default build stays valid HTML. Its numbers can still be re-run.

## Open questions

- Preact with `preact/compat` (a few KB gzip) instead of React DOM for islands could cut most of the 65 KB island floor. That needs its own spike with measurements: OpenUI's runtime and Radix would still need React on the server, and islands would have to hydrate with Preact.
- Island hydration strategy per component type (visible, idle, on interaction) is left to the component library. The spike uses visible with a 200px margin.
