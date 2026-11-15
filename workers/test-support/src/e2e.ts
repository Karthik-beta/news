// End-to-end launcher: starts one Worker from its real wrangler.jsonc with Wrangler's test harness (workerd under
// Miniflare, fully local) and returns a real http://127.0.0.1:<port> URL to hit with fetch.
//
// Must run under Node. Wrangler's harness cannot be reached over HTTP from Bun, because Bun's fetch ignores the
// undici dispatcher Miniflare relies on; vitest runs under Node, which is what we want here.
import { createTestHarness } from "wrangler";

export interface RunningWorker {
  /** Base URL of the running Worker, e.g. http://127.0.0.1:53211/ */
  url: URL;
  /** Real HTTP request to the Worker. `path` is resolved against `url`. */
  get(path: string, init?: RequestInit): Promise<Response>;
  close(): Promise<void>;
}

export async function startWorker(
  root: string = process.env.NEWS_WORKER_ROOT ?? process.cwd(),
): Promise<RunningWorker> {
  if (process.versions.bun) {
    throw new Error(
      "E2E tests must run under Node (vitest does), not Bun: Wrangler's harness is unreachable from Bun.",
    );
  }
  // Local only: no telemetry, no banner.
  process.env.WRANGLER_SEND_METRICS = "false";
  process.env.WRANGLER_SEND_ERROR_REPORTS = "false";
  process.env.WRANGLER_HIDE_BANNER = "true";

  const server = createTestHarness({ root, workers: [{ configPath: "./wrangler.jsonc" }] });
  const { url } = await server.listen();
  return {
    url,
    get: (path, init) => fetch(new URL(path, url), init),
    close: () => server.close(),
  };
}
