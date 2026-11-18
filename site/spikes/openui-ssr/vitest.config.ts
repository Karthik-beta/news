/// <reference types="vitest/config" />
import { getViteConfig } from "astro/config";

// Only for the spike's Container API measurement; not part of `bun run test`.
export default getViteConfig({
  test: {
    pool: "threads",
    environment: "node",
    include: ["spikes/openui-ssr/**/*.bench.test.ts"],
  },
});
