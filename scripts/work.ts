// `bun run work`: the optional Dispatch background worker, `news work --pools net,cpu,docling` (Bureau §5.6, D8).
// It runs downloaded documents through code (Wire, Clerk, Docling), so it gets only the variables it needs: the
// database and telemetry settings below, plus the few the operating system needs to start a process. Nothing else
// from .env reaches it (the write-API key, AWS and Cloudflare keys, the Postgres password outside DATABASE_URL).
// Runs under `bun --no-env-file`: .env is read here, literally, and the shell's exported values win, as in compose.ts.
// The binary is the one `bun run setup` builds (`cargo build`, cli/target/debug); NEWS_WORK_BIN overrides it.
import { spawnSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { composeEnv, noEnvFileProblem } from "./env-file";
import { root } from "./lib";

/** Variables `news work` may see, by exact name. */
export const WORK_ENV = ["DATABASE_URL", "NEWS_DB_SCHEMA", "NEWS_OTEL_HTTP", "NEWS_LOG", "NEWS_TELEMETRY"] as const;
/** Prefixes `news work` may see: the ClickHouse telemetry settings (NEWS_CH_HTTP, NEWS_CH_USER, NEWS_CH_PASSWORD). */
export const WORK_ENV_PREFIXES = ["NEWS_CH_"] as const;
/** What the OS needs to start a process and open sockets (Windows needs SystemRoot), and the worker id's host name. */
export const OS_ENV = ["PATH", "SYSTEMROOT", "WINDIR", "TEMP", "TMP", "COMPUTERNAME", "HOSTNAME"];

const allowed = (key: string) =>
  (WORK_ENV as readonly string[]).includes(key) ||
  WORK_ENV_PREFIXES.some((p) => key.startsWith(p)) ||
  // Windows environment names are case-insensitive (Path, SystemRoot).
  OS_ENV.includes(key.toUpperCase());

/** The environment for `news work`: .env values with the shell's laid over them, filtered to the allow-list. */
export function workEnv(dotenv: string, shell: Record<string, string | undefined>): Record<string, string> {
  return Object.fromEntries(Object.entries(composeEnv(dotenv, shell)).filter(([k]) => allowed(k)));
}

/** The `news` binary `bun run setup` builds. */
export function workBinary(env: Record<string, string | undefined>, platform = process.platform): string {
  return env.NEWS_WORK_BIN || join(root, "cli/target/debug", platform === "win32" ? "news.exe" : "news");
}

/** `news work` arguments: the code-only pools unless the caller names others (news work refuses ai, render, publish). */
export function workArgs(argv: string[]): string[] {
  return [
    "work",
    ...(argv.some((a) => a === "--pools" || a.startsWith("--pools=")) ? [] : ["--pools", "net,cpu,docling"]),
    ...argv,
  ];
}

export type Spawn = (cmd: string, args: string[], env: Record<string, string>) => number;

const spawnInherit: Spawn = (cmd, args, env) => spawnSync(cmd, args, { cwd: root, env, stdio: "inherit" }).status ?? 1;

/** Start the worker; returns its exit code. */
export function main(
  argv: string[],
  shell: Record<string, string | undefined>,
  spawn: Spawn = spawnInherit,
  dotenvPath = join(root, ".env"),
): number {
  const env = workEnv(existsSync(dotenvPath) ? readFileSync(dotenvPath, "utf8") : "", shell);
  const bin = workBinary(shell);
  if (!existsSync(bin)) {
    console.error(`✗ ${bin} not found: run \`bun run setup\` (or \`cargo build\` in cli/).`);
    return 1;
  }
  // Names only, never values: what the worker can see.
  console.error(`news work environment: ${Object.keys(env).sort().join(", ")}`);
  return spawn(bin, workArgs(argv), env);
}

if (import.meta.main) {
  const problem = noEnvFileProblem(process.execArgv, "scripts/work.ts");
  if (problem) {
    console.error(`✗ ${problem}`);
    process.exit(1);
  }
  process.exit(main(process.argv.slice(2), process.env));
}
