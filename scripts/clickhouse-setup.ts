// Creates the isolated `news` database, user, settings profile and quota in the shared ClickHouse,
// then the tables (002, 003). Idempotent. The password comes from .env and is passed on stdin, not argv.
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { applyAccess, passwordProblem, renderAccessSql, splitStatements } from "./clickhouse";
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

// The tables, as user news: the events table (002), then Ticker's columns and tables (003). Idempotent.
for (const file of ["002_events.sql", "003_ticker.sql"]) {
  for (const statement of splitStatements(readFileSync(join(root, "infra/clickhouse", file), "utf8"))) {
    const ddl = await chQuery(statement);
    if (!ddl.ok) {
      console.error(`${file} failed (as user news):\n${ddl.body}`);
      process.exit(1);
    }
  }
  console.log(`✓ ${file} applied (as user news)`);
}
