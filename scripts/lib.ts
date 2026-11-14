// Shared helpers for the local setup scripts. Run with Bun from the repo root (Bun loads .env automatically).
import { type SpawnSyncOptions, spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

export const root = fileURLToPath(new URL("..", import.meta.url));

export function run(cmd: string, args: string[], opts: SpawnSyncOptions = {}) {
  const r = spawnSync(cmd, args, { encoding: "utf8", cwd: root, ...opts });
  return {
    ok: r.status === 0,
    status: r.status,
    out: (r.stdout ?? "").toString().trim(),
    err: (r.stderr ?? "").toString().trim(),
  };
}

export function env(name: string, fallback = ""): string {
  return process.env[name] ?? fallback;
}

/** Query ClickHouse over HTTP as the news user. */
export async function chQuery(sql: string): Promise<{ ok: boolean; body: string }> {
  const url = env("NEWS_CH_HTTP", "http://127.0.0.1:8123");
  const auth = btoa(`${env("NEWS_CH_USER", "news")}:${env("NEWS_CH_PASSWORD")}`);
  try {
    const res = await fetch(url, { method: "POST", body: sql, headers: { authorization: `Basic ${auth}` } });
    return { ok: res.ok, body: (await res.text()).trim() };
  } catch (e) {
    return { ok: false, body: String(e) };
  }
}

export const mark = (ok: boolean | "warn") => (ok === "warn" ? "!" : ok ? "✓" : "✗");
