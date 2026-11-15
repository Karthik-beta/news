// One-command local setup: `bun run setup`. Safe to re-run.
// 1. Creates .env from .env.example with generated local passwords (never overwrites an existing value).
// 2. Starts PostgreSQL + pgvector, the OTel Collector and DynamoDB Local (for the Worker tests).
// 3. Creates the isolated `news` database and user in the shared ClickHouse.
// 4. Installs JS dependencies, syncs design tokens, builds the Rust CLI.
// 5. Runs the doctor with the end-to-end telemetry check.

import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { getEnvValue, setEnvValue, withLocalSecrets } from "./env-file";
import { root, run } from "./lib";

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
  if (!getEnvValue(text, k)) text = setEnvValue(text, k, getEnvValue(example, k));
}
text = withLocalSecrets(text);
writeFileSync(envPath, text);
console.log("✓ .env ready (local passwords generated where empty)");

// Bun loaded .env before this script ran; reload values for the child processes.
for (const k of ["NEWS_PG_PASSWORD", "NEWS_CH_PASSWORD", "DATABASE_URL"]) process.env[k] = getEnvValue(text, k);

const step = (label: string, cmd: string, args: string[], cwd = root) => {
  process.stdout.write(`… ${label}\n`);
  const r = run(cmd, args, { cwd, stdio: "inherit", env: process.env });
  if (!r.ok) {
    console.error(`✗ ${label} failed`);
    process.exit(1);
  }
  console.log(`✓ ${label}`);
};

step("start PostgreSQL, OTel Collector and DynamoDB Local", "docker", [
  "compose",
  "-f",
  "infra/docker-compose.yml",
  "--env-file",
  ".env",
  "up",
  "-d",
  "--wait",
]);
step("ClickHouse news database and user", "bun", ["scripts/clickhouse-setup.ts"]);
step("install JS dependencies", "bun", ["install"]);
step("sync design tokens", "bun", ["site/scripts/sync-tokens.ts"]);
step("build the Rust CLI", "cargo", ["build"], join(root, "cli"));
step("doctor (with end-to-end telemetry check)", "bun", ["scripts/doctor.ts", "--e2e"]);
