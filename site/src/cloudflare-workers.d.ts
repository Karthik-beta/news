// The Worker's bindings, as far as the site reads them (typed as SiteEnv in src/data/index.ts). The site does not pull in
// @cloudflare/workers-types: its globals collide with the DOM types the components use.
declare module "cloudflare:workers" {
  export const env: Record<string, unknown>;
}

/** True only in a build made with NEWS_PREVIEW=1 (astro.config.mjs `vite.define`); the preview code is dead code otherwise. */
declare const __NEWS_PREVIEW__: boolean;
