// Types for the tests: the bindings Miniflare provides (see vitest.integration.config.ts) and the Worker's main
// module, which `exports.default` in the tests needs. `wrangler types` would generate the same; the real bindings
// are declared in wrangler.jsonc once Karthik adds the real resource IDs.
declare namespace Cloudflare {
  interface GlobalProps {
    mainModule: typeof import("../src/index");
  }
  interface Env {
    SEARCH_DB: D1Database;
    SEARCH_2026: D1Database;
    TEST_MIGRATIONS: import("cloudflare:test").D1Migration[];
    SEARCH_MIGRATIONS: import("cloudflare:test").D1Migration[];
  }
}
