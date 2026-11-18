# Spike: OpenUI server rendering

The full write-up is [ADR 0001](../adr/0001-openui-server-rendering.md). The code and re-run steps are in `site/spikes/openui-ssr/`.

- **Question:** do OpenUI Lang pages render to static HTML under `bun --bun`, with one chart island hydrating and 0 KB JS for the static parts?
- **Built:** a four-component OpenUI library and fixture page, rendered three ways: Astro Container API with an Astro island; `react-dom/server` over OpenUI's `Renderer.Root` with our own `data-island` hydrator; and our own walker over the parse tree. There's also a Playwright test in Chromium.
- **Result:** all three render complete HTML. Static parts are never hydrated, and the island hydrates with no warnings. React server rendering takes 4.0 / 12.7 ms (p50 / p95) and produces 1.0 KB gzip of HTML. Astro takes 8.9 / 14.7 ms and 3.3 KB, and its HTML is invalid. JS shipped is about 65–70 KB gzip either way, almost all of it React DOM.
- **Recommendation:** React server rendering with `Renderer.Root` and our own islands. The own walker is the proven fallback.
- **Risks:** OpenUI moves fast (pin exactly), `react-ui` components were not checked for SSR, and the React DOM island floor is about 65 KB.
