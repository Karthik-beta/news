// @ts-check

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

// Static article pages are rendered on the laptop; dynamic hubs move to an SSR Worker in Phase 6.
export default defineConfig({
  site: "https://news.karthik.nz",
  output: "static",
  integrations: [react(), fixturePages()],
  server: { host: "127.0.0.1", port: 4321 },
});
