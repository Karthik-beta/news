// Runs the Rust CLI's tests at one level. Usage (from the repo root): `bun scripts/test-cli.ts <level>`
//   unit         in-crate tests, no services
//   e2e          the real `news` binary (cli/crates/news/tests/cli.rs), no services
//   integration  Postgres tests (cli/crates/news/tests/postgres.rs); needs `bun run infra:up`. The root
//                `bun run test:integration` runs this level after the Bun workspaces' integration tests.
// Bun loads .env, so DATABASE_URL is available here; cargo does not read .env, so it is passed on explicitly.
import { spawnSync } from "node:child_process";
import { connect } from "node:net";
import { join } from "node:path";
import { root } from "./lib";

export const levels = ["unit", "e2e", "integration"] as const;
export type Level = (typeof levels)[number];

export function cargoArgs(level: Level): string[] {
  switch (level) {
    case "unit":
      return ["test", "--locked", "--workspace", "--lib", "--bins"];
    case "e2e":
      return ["test", "--locked", "-p", "news", "--test", "cli"];
    case "integration":
      return ["test", "--locked", "-p", "news", "--test", "postgres", "--", "--ignored"];
  }
}

/** The database the Postgres tests use: NEWS_TEST_DATABASE_URL, else the local DATABASE_URL. Empty counts as unset. */
export function testDatabaseUrl(environ: Record<string, string | undefined>): string | undefined {
  return environ.NEWS_TEST_DATABASE_URL || environ.DATABASE_URL || undefined;
}

/** Host and port of a postgres:// URL, or undefined when it does not parse. Never includes the password. */
export function hostPort(url: string): { host: string; port: number } | undefined {
  try {
    const u = new URL(url);
    if (!/^postgres(ql)?:$/.test(u.protocol) || !u.hostname) return undefined;
    // libpq and the Rust driver honour `?port=` over the authority port, so the preflight does too.
    const port = u.searchParams.get("port") || u.port || "5432";
    return { host: u.hostname.replace(/^\[|\]$/g, ""), port: Number(port) };
  } catch {
    return undefined;
  }
}

export function canConnect(host: string, port: number, timeoutMs = 3000): Promise<boolean> {
  return new Promise((resolve) => {
    const socket = connect({ host, port });
    const done = (ok: boolean) => {
      socket.destroy();
      resolve(ok);
    };
    socket.setTimeout(timeoutMs, () => done(false));
    socket.once("connect", () => done(true));
    socket.once("error", () => done(false));
  });
}

/** Check the integration prerequisites. Returns a one-line problem, or the URL to use. Never echoes the URL. */
export async function preflight(
  environ: Record<string, string | undefined>,
  reachable: (host: string, port: number) => Promise<boolean> = canConnect,
): Promise<{ url: string } | { problem: string }> {
  const url = testDatabaseUrl(environ);
  if (!url) return { problem: "No database URL: run `bun run setup`, then `bun run infra:up`." };
  const target = hostPort(url);
  if (!target)
    return { problem: "NEWS_TEST_DATABASE_URL / DATABASE_URL is not a valid postgres:// URL (value not shown)." };
  if (!(await reachable(target.host, target.port)))
    return { problem: `Postgres is not reachable at ${target.host}:${target.port}: run \`bun run infra:up\`.` };
  return { url };
}

if (import.meta.main) {
  const level = process.argv[2] as Level;
  if (!levels.includes(level)) {
    console.error(`Usage: bun scripts/test-cli.ts <${levels.join("|")}>`);
    process.exit(2);
  }
  const childEnv: NodeJS.ProcessEnv = { ...process.env };
  if (level === "integration") {
    const pre = await preflight(process.env);
    if ("problem" in pre) {
      console.error(pre.problem);
      process.exit(1);
    }
    childEnv.NEWS_TEST_DATABASE_URL = pre.url;
  }
  const r = spawnSync("cargo", cargoArgs(level), { cwd: join(root, "cli"), env: childEnv, stdio: "inherit" });
  process.exit(r.status ?? 1);
}
