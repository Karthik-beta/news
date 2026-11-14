// @ts-check

import react from "@astrojs/react";
import { defineConfig } from "astro/config";

// Static article pages are rendered on the laptop; dynamic hubs move to an SSR Worker in Phase 6.
export default defineConfig({
  site: "https://news.karthik.nz",
  output: "static",
  integrations: [react()],
  server: { host: "127.0.0.1", port: 4321 },
});
