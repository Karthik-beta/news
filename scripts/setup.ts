// One-command local setup: `bun run setup`. Safe to re-run.
// 1. Creates .env from .env.example with generated local passwords (never overwrites an existing value).
// 2. Starts PostgreSQL + pgvector, the OTel Collector and DynamoDB Local (for the Worker tests).
// 3. Creates the isolated `news` database and user in the shared ClickHouse.
// 4. Installs JS dependencies, syncs design tokens, builds the Rust CLI, applies migrations, enables the git hooks.
// 5. Runs the doctor with the end-to-end telemetry check.

import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { getEnvValue, noEnvFileProblem, reloadChildEnv, setEnvValue, withLocalSecrets } from "./env-file";
import { root, run } from "./lib";

// `bun run setup` runs this under --no-env-file, so process.env is the shell's own environment (see composeEnv).
const notReady = noEnvFileProblem(process.execArgv, "scripts/setup.ts");
if (notReady) {
  console.error(`✗ ${notReady}`);
  process.exit(1);
}

const envPath = join(root, ".env");
const examplePath = join(root, ".env.example");

// Non-secret settings added after a .env was first created (DynamoDB Local, dummy credentials): copy from the example.
const example = readFileSync(examplePath, "utf8");
let text = readFileSync(existsSync(envPath) ? envPath : examplePath, "utf8");
for (const k of [
  "NEWS_DYNAMODB_PORT",
  "NEWS_DYNAMODB_ENDPOINT",
  "NEWS_DYNAMODB_REGION",
  "NEWS_DYNAMODB_ACCESS_KEY_ID",
  "NEWS_DYNAMODB_SECRET_ACCESS_KEY",
]) {
  if (getEnvValue(text, k)) continue;
  // A missing endpoint follows the configured port, so a custom NEWS_DYNAMODB_PORT stays reachable.
  const value =
    k === "NEWS_DYNAMODB_ENDPOINT"
      ? `http://127.0.0.1:${getEnvValue(text, "NEWS_DYNAMODB_PORT") || "8000"}`
      : getEnvValue(example, k);
  text = setEnvValue(text, k, value);
}
text = withLocalSecrets(text);
writeFileSync(envPath, text);
console.log("✓ .env ready (local passwords generated where empty)");

// Bun did not load .env (--no-env-file); hand the final values to the child processes, unless exported in the shell.
reloadChildEnv(text, process.env);

const step = (label: string, cmd: string, args: string[], cwd = root) => {
  process.stdout.write(`… ${label}\n`);
  const r = run(cmd, args, { cwd, stdio: "inherit", env: process.env });
  if (!r.ok) {
    console.error(`✗ ${label} failed`);
    process.exit(1);
  }
  console.log(`✓ ${label}`);
};

// Through the wrapper, so a `$` in a .env value reaches the containers as typed.
step("start PostgreSQL, OTel Collector and DynamoDB Local", "bun", [
  "--no-env-file",
  "scripts/compose.ts",
  "up",
  "-d",
  "--wait",
]);
step("ClickHouse news database and user", "bun", ["scripts/clickhouse-setup.ts"]);
step("install JS dependencies", "bun", ["install"]);
step("sync design tokens", "bun", ["site/scripts/sync-tokens.ts"]);
step("build the Rust CLI", "cargo", ["build"], join(root, "cli"));
step("PostgreSQL migrations (news db migrate)", join(root, "cli/target/debug/news"), ["db", "migrate"]);
// The local gitleaks pre-commit hook (.githooks/pre-commit); it skips with a note when gitleaks is not installed.
step("git hooks (gitleaks pre-commit)", "git", ["config", "core.hooksPath", ".githooks"]);
step("doctor (with end-to-end telemetry check)", "bun", ["scripts/doctor.ts", "--e2e"]);
