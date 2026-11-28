// Shared helpers for the local setup scripts. Run with Bun from the repo root (Bun loads .env automatically).
import { type SpawnSyncOptions, spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

export const root = fileURLToPath(new URL("..", import.meta.url));

export function run(cmd: string, args: string[], opts: SpawnSyncOptions = {}) {
  const r = spawnSync(cmd, args, { encoding: "utf8", cwd: root, ...opts });
  const stderr = (r.stderr ?? "").toString().trim();
  return {
    ok: r.status === 0,
    status: r.status,
    out: (r.stdout ?? "").toString().trim(),
    // A command that could not start (e.g. `docker` not installed) has no stderr; report why instead.
    err: [stderr, r.error?.message ?? ""].filter(Boolean).join("\n"),
  };
}

/** An environment variable, or the fallback when it is unset or empty (`KEY=` in .env). */
export function env(name: string, fallback = ""): string {
  return process.env[name] || fallback;
}

/** Query ClickHouse over HTTP as the news user. */
export async function chQuery(sql: string): Promise<{ ok: boolean; body: string }> {
  const url = env("NEWS_CH_HTTP", "http://127.0.0.1:8123");
  try {
    // UTF-8 Basic auth: btoa would throw on any character outside Latin-1.
    const auth = Buffer.from(`${env("NEWS_CH_USER", "news")}:${env("NEWS_CH_PASSWORD")}`, "utf8").toString("base64");
    const res = await fetch(url, { method: "POST", body: sql, headers: { authorization: `Basic ${auth}` } });
    return { ok: res.ok, body: (await res.text()).trim() };
  } catch (e) {
    return { ok: false, body: String(e) };
  }
}

export const mark = (ok: boolean | "warn") => (ok === "warn" ? "!" : ok ? "✓" : "✗");

/** The shorts project's Docker network; news-otel joins it to reach the shared ClickHouse (infra/docker-compose.yml). */
export const SHORTS_NETWORK = "shorts_default";

type Runner = (cmd: string, args: string[]) => { ok: boolean; err: string };

/**
 * Why `infra:up` cannot attach news-otel to the shorts network, or null when it can. Compose's own error for a missing
 * external network does not say to start the shorts stack, so this is checked first.
 */
export function shortsNetworkProblem(runner: Runner = run): string | null {
  const r = runner("docker", ["network", "inspect", "--format", "{{.Name}}", SHORTS_NETWORK]);
  if (r.ok) return null;
  if (/no such network|not found/i.test(r.err))
    return `Docker network ${SHORTS_NETWORK} not found: start the shorts stack first (it owns the shared ClickHouse that news-otel writes to)`;
  return `could not check Docker network ${SHORTS_NETWORK}: ${r.err.split("\n")[0] || "docker failed"}`;
}
