// Types for the tests: the bindings Miniflare provides (see vitest.integration.config.ts) and the Worker's main
// module, which `exports.default` in the tests needs. `wrangler types` would generate the same; the real bindings
// are declared in wrangler.jsonc once Karthik adds the real resource IDs.
declare namespace Cloudflare {
  interface GlobalProps {
    mainModule: typeof import("../src/index");
  }
  interface Env {
    PAGES: R2Bucket;
    NEWS_DYNAMODB_ENDPOINT: string;
    NEWS_DYNAMODB_REGION: string;
    NEWS_DYNAMODB_ACCESS_KEY_ID: string;
    NEWS_DYNAMODB_SECRET_ACCESS_KEY: string;
  }
}
