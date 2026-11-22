// @ts-check

import cloudflare from "@astrojs/cloudflare";
import react from "@astrojs/react";
import { defineConfig } from "astro/config";

/**
 * Fixture pages live in src/pages/_fixtures/. Astro never routes an underscore folder on its own, so this
 * integration adds them, at /fixtures/…, only for test and CI builds (NEWS_FIXTURES=1). A production build never
 * sees them, so none of their code or styles reaches dist/. The OpenUI spike's routes (ADR 0001) need OPENUI_SPIKE=1.
 * @returns {import("astro").AstroIntegration}
 */
function fixturePages() {
  return {
    name: "news-fixture-pages",
    hooks: {
      "astro:config:setup": ({ injectRoute }) => {
        if (process.env.NEWS_FIXTURES === "1") {
          const dir = "./src/pages/_fixtures";
          injectRoute({ pattern: "/fixtures/renderer/[bundle]", entrypoint: `${dir}/renderer/[bundle]/index.astro` });
          injectRoute({
            pattern: "/fixtures/renderer/[bundle]/[...file]",
            entrypoint: `${dir}/renderer/[bundle]/[...file].ts`,
          });
          injectRoute({ pattern: "/fixtures/primitives/[name]", entrypoint: `${dir}/primitives/[name].astro` });
          injectRoute({
            pattern: "/fixtures/components/[name]/[fixture]",
            entrypoint: `${dir}/components/[name]/[fixture]/index.astro`,
          });
          injectRoute({
            pattern: "/fixtures/components/[name]/[fixture]/[...file]",
            entrypoint: `${dir}/components/[name]/[fixture]/[...file].ts`,
          });
          injectRoute({ pattern: "/fixtures/images/[file]", entrypoint: `${dir}/images/[file].ts` });
        }
        if (process.env.OPENUI_SPIKE === "1") {
          injectRoute({
            pattern: "/fixtures/openui-ssr/[approach]",
            entrypoint: "./src/pages/_fixtures/openui-ssr/[approach].astro",
          });
        }
      },
    },
  };
}

// Static article pages are rendered on the laptop and served from R2. This is the `site` Worker (ADR 0010): pages
// that list pages declare `prerender = false` and render on request; everything else is prerendered at build, in
// Node (the renderer and the fixtures read files). No sessions: the adapter would add a KV binding for them.
export default defineConfig({
  site: "https://news.karthik.nz",
  output: "static",
  // Vitest compiles .astro and .tsx through this config; the adapter's workerd plugin has no place there.
  adapter: process.env.VITEST ? undefined : cloudflare({ prerenderEnvironment: "node", imageService: "passthrough" }),
  session: false,
  integrations: [react(), fixturePages()],
  server: { host: "127.0.0.1", port: 4321 },
});
