// Shared test helpers. Bun loads the developer's real .env before tests start, so every test that reads
// the environment sets exactly what it needs and restores the rest; no test depends on .env contents.
import { cpSync, mkdirSync, mkdtempSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { root } from "../lib";

type Vars = Record<string, string | undefined>;

/** Run `fn` with the given environment variables set (undefined deletes), then restore the originals. */
export async function withEnv<T>(vars: Vars, fn: () => T | Promise<T>): Promise<T> {
  const saved: Vars = {};
  for (const [k, v] of Object.entries(vars)) {
    saved[k] = process.env[k];
    if (v === undefined) delete process.env[k];
    else process.env[k] = v;
  }
  try {
    return await fn();
  } finally {
    for (const [k, v] of Object.entries(saved)) {
      if (v === undefined) delete process.env[k];
      else process.env[k] = v;
    }
  }
}

export function tempDir(prefix = "news-scripts-test-"): string {
  return mkdtempSync(join(tmpdir(), prefix));
}

export function removeDir(dir: string) {
  rmSync(dir, { recursive: true, force: true });
}

/**
 * A throwaway copy of the repo's `scripts/`, `.env.example` and the ClickHouse SQL, so a script can be run
 * end to end with its `root` pointing at a temp directory. The compose file is deliberately not copied: even
 * if a script got past a guard and found `docker`, there would be no compose file to act on.
 */
export function makeScratchRepo(files: Record<string, string> = {}): string {
  const dir = tempDir();
  mkdirSync(join(dir, "scripts"));
  mkdirSync(join(dir, "infra"));
  for (const f of readdirSync(join(root, "scripts"))) {
    if (f.endsWith(".ts")) cpSync(join(root, "scripts", f), join(dir, "scripts", f));
  }
  cpSync(join(root, ".env.example"), join(dir, ".env.example"));
  cpSync(join(root, "infra/clickhouse"), join(dir, "infra/clickhouse"), { recursive: true });
  for (const [name, content] of Object.entries(files)) writeFileSync(join(dir, name), content);
  return dir;
}

/**
 * Environment for a child `bun` process: nothing from the developer's .env, and a PATH that holds no tools,
 * so `docker`, `cargo` and friends cannot be found and nothing real can ever be touched.
 */
export function sandboxEnv(scratchRepo: string, extra: Vars = {}): Record<string, string> {
  const emptyBin = join(scratchRepo, ".empty-bin");
  mkdirSync(emptyBin, { recursive: true });
  const keep = ["SystemRoot", "windir", "TEMP", "TMP", "TMPDIR", "HOME", "USERPROFILE", "LOCALAPPDATA", "APPDATA"];
  const env: Record<string, string> = { PATH: emptyBin };
  for (const k of keep) if (process.env[k] !== undefined) env[k] = process.env[k] as string;
  for (const [k, v] of Object.entries(extra)) if (v !== undefined) env[k] = v;
  return env;
}

export function runBun(args: string[], opts: { cwd: string; env: Record<string, string>; input?: string }) {
  const r = Bun.spawnSync([process.execPath, ...args], {
    cwd: opts.cwd,
    env: opts.env,
    stdin: opts.input === undefined ? undefined : new TextEncoder().encode(opts.input),
  });
  return { status: r.exitCode, out: r.stdout.toString(), err: r.stderr.toString() };
}
