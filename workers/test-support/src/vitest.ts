// Vitest config factories shared by the three Workers (api, search, mcp). Each worker keeps three tiny config files
// that call these, so the test levels behave the same everywhere:
//
//   unitConfig         `test`              src/**/*.test.ts       inside workerd, wrangler.jsonc bindings only, no services
//   integrationConfig  `test:integration`  test/integration/**    inside workerd + Miniflare D1/R2, DynamoDB Local if asked
//   e2eConfig          `test:e2e`          test/e2e/**            plain Node, starts the Worker with Wrangler, real HTTP
//
// Cloudflare bindings are provided here by Miniflare (the official local simulator), never by production IDs, so
// everything runs on a laptop or in CI with no Cloudflare account.
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { cloudflareTest, type D1Migration, readD1Migrations } from "@cloudflare/vitest-plugin";
import { defineConfig } from "vitest/config";
import { dynamoBindings, dynamoConfigFromEnv } from "./dynamodb";
import { repoEnv } from "./repo-env";

const MIGRATIONS_DIR = fileURLToPath(new URL("../d1-migrations", import.meta.url));
const DYNAMODB_GLOBAL_SETUP = fileURLToPath(new URL("./global-setup-dynamodb.ts", import.meta.url));

export interface WorkerTestOptions {
  /** The worker's package directory (`import.meta.dirname` in its vitest config). */
  root: string;
}

export interface IntegrationOptions extends WorkerTestOptions {
  /** D1 binding names to create (empty in-memory databases). Also injects `TEST_MIGRATIONS`. */
  d1?: string[];
  /** R2 bucket binding names to create. */
  r2?: string[];
  /** Inject the DynamoDB Local settings as Worker bindings and fail fast when the service is down. */
  dynamodb?: boolean;
}

const wranglerConfig = (root: string) => ({ configPath: join(root, "wrangler.jsonc") });

export function unitConfig({ root }: WorkerTestOptions) {
  return defineConfig({
    root,
    plugins: [cloudflareTest({ wrangler: wranglerConfig(root) })],
    test: { include: ["src/**/*.test.ts"] },
  });
}

export function integrationConfig({ root, d1 = [], r2 = [], dynamodb = false }: IntegrationOptions) {
  return defineConfig(async () => {
    const bindings: Record<string, string | D1Migration[]> = {};
    if (dynamodb) Object.assign(bindings, dynamoBindings(dynamoConfigFromEnv(repoEnv())));
    if (d1.length > 0) bindings.TEST_MIGRATIONS = await readD1Migrations(MIGRATIONS_DIR);
    return {
      root,
      plugins: [
        cloudflareTest({
          wrangler: wranglerConfig(root),
          miniflare: { d1Databases: d1, r2Buckets: r2, bindings },
        }),
      ],
      test: {
        include: ["test/integration/**/*.test.ts"],
        globalSetup: dynamodb ? [DYNAMODB_GLOBAL_SETUP] : [],
        testTimeout: 20_000,
      },
    };
  });
}

export function e2eConfig({ root }: WorkerTestOptions) {
  return defineConfig({
    root,
    test: {
      include: ["test/e2e/**/*.test.ts"],
      environment: "node",
      env: { NEWS_WORKER_ROOT: root },
      // Starting Wrangler takes a few seconds; files run one at a time so ports and workerd stay calm.
      testTimeout: 30_000,
      hookTimeout: 90_000,
      fileParallelism: false,
    },
  });
}
