// One-command local setup: `bun run setup`. Safe to re-run.
// 1. Creates .env from .env.example with generated local passwords (never overwrites an existing value).
// 2. Starts PostgreSQL + pgvector, the OTel Collector and DynamoDB Local (for the Worker tests).
// 3. Creates the isolated `news` database and user in the shared ClickHouse.
// 4. Installs JS dependencies, syncs design tokens, builds the Rust CLI.
// 5. Runs the doctor with the end-to-end telemetry check.

import { randomBytes } from "node:crypto";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { root, run } from "./lib";

const envPath = join(root, ".env");
const examplePath = join(root, ".env.example");
const secret = () => randomBytes(24).toString("base64url");

let text = existsSync(envPath) ? readFileSync(envPath, "utf8") : readFileSync(examplePath, "utf8");
const get = (k: string) => text.match(new RegExp(`^${k}=(.*)$`, "m"))?.[1]?.trim() ?? "";
const set = (k: string, v: string) => {
  text = new RegExp(`^${k}=`, "m").test(text)
    ? text.replace(new RegExp(`^${k}=.*$`, "m"), `${k}=${v}`)
    : `${text.trimEnd()}\n${k}=${v}\n`;
};

// Non-secret settings added after a .env was first created (DynamoDB Local, dummy credentials): copy from the example.
const example = readFileSync(examplePath, "utf8");
for (const k of [
  "NEWS_DYNAMODB_PORT",
  "NEWS_DYNAMODB_ENDPOINT",
  "NEWS_DYNAMODB_REGION",
  "NEWS_DYNAMODB_ACCESS_KEY_ID",
  "NEWS_DYNAMODB_SECRET_ACCESS_KEY",
]) {
  if (!get(k)) set(k, example.match(new RegExp(`^${k}=(.*)$`, "m"))?.[1]?.trim() ?? "");
}

if (!get("NEWS_PG_PASSWORD")) set("NEWS_PG_PASSWORD", secret());
if (!get("NEWS_CH_PASSWORD")) set("NEWS_CH_PASSWORD", secret());
if (!get("DATABASE_URL"))
  set("DATABASE_URL", `postgres://news:${get("NEWS_PG_PASSWORD")}@127.0.0.1:${get("NEWS_PG_PORT") || "5433"}/news`);
writeFileSync(envPath, text);
console.log("✓ .env ready (local passwords generated where empty)");

// Bun loaded .env before this script ran; reload values for the child processes.
for (const k of ["NEWS_PG_PASSWORD", "NEWS_CH_PASSWORD", "DATABASE_URL"]) process.env[k] = get(k);

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
