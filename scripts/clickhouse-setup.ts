// Creates the isolated `news` database, user, settings profile and quota in the shared ClickHouse,
// then the events table. Idempotent. The password comes from .env and is passed on stdin, not argv.
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { chQuery, env, root, run } from "./lib";

const container = env("NEWS_CH_CONTAINER", "shorts-clickhouse-1");
const password = env("NEWS_CH_PASSWORD");
if (!password) {
  console.error("NEWS_CH_PASSWORD is empty. Run `bun run setup` first.");
  process.exit(1);
}
if (/['\\]/.test(password)) {
  console.error("NEWS_CH_PASSWORD must not contain quotes or backslashes.");
  process.exit(1);
}

const access = readFileSync(join(root, "infra/clickhouse/001_access.sql"), "utf8").replaceAll(
  "{{NEWS_CH_PASSWORD}}",
  password,
);
const admin = run("docker", ["exec", "-i", container, "clickhouse-client", "--multiquery"], { input: access });
if (!admin.ok) {
  console.error(`Access setup failed in ${container}:\n${admin.err}`);
  process.exit(1);
}
console.log(`✓ database, user, profile and quota ready in ${container}`);

const events = readFileSync(join(root, "infra/clickhouse/002_events.sql"), "utf8");
const ddl = await chQuery(events);
if (!ddl.ok) {
  console.error(`events table failed (as user news):\n${ddl.body}`);
  process.exit(1);
}
console.log("✓ news.events ready (created as user news)");
