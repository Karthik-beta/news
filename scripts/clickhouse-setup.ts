// Creates the isolated `news` database, user, settings profile and quota in the shared ClickHouse,
// then the events table. Idempotent. The password comes from .env and is passed on stdin, not argv.
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { applyAccess, passwordProblem, renderAccessSql } from "./clickhouse";
import { chQuery, env, root } from "./lib";

const container = env("NEWS_CH_CONTAINER", "shorts-clickhouse-1");
const password = env("NEWS_CH_PASSWORD");
const problem = passwordProblem(password);
if (problem) {
  console.error(problem);
  process.exit(1);
}

const access = renderAccessSql(readFileSync(join(root, "infra/clickhouse/001_access.sql"), "utf8"), password);
const admin = applyAccess(container, access);
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
