// Types for the tests: the binding Miniflare provides (see vitest.integration.config.ts) and the Worker's main
// module, which `exports.default` in the tests needs. `wrangler types` would generate the same; the real binding
// is declared in wrangler.jsonc once Karthik adds the real resource ID.
declare namespace Cloudflare {
  interface GlobalProps {
    mainModule: typeof import("../src/index");
  }
  interface Env {
    SEARCH_DB: D1Database;
    TEST_MIGRATIONS: import("cloudflare:test").D1Migration[];
  }
}
