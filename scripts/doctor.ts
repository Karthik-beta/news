// `bun run doctor` checks the local development environment. `--e2e` also proves telemetry reaches ClickHouse:
// it writes a test row to news.events and sends an OTLP log through the collector into news.otel_logs.
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { SQL } from "bun";
import {
  DynamoDbLocal,
  dynamoConfigFromEnv,
  fromItem,
  toItem,
  withTempTable,
} from "../workers/test-support/src/dynamodb";
import { requireDynamoDb } from "../workers/test-support/src/service-check";
import { expandedKeys } from "./env-file";
import { chQuery, env, mark, root, run } from "./lib";

const e2e = process.argv.includes("--e2e");
let failures = 0;
const line = (ok: boolean | "warn", label: string, detail = "") => {
  if (ok === false) failures++;
  console.log(`${mark(ok)} ${label}${detail ? `  ${detail}` : ""}`);
};
const version = (cmd: string, args = ["--version"]) => {
  const r = run(cmd, args, { shell: process.platform === "win32" });
  return r.ok ? (r.out || r.err).split("\n")[0] : "";
};

// Bun expands `$` in .env values (even single-quoted), so a key with one reads differently in every Bun-run tool.
const envPath = join(root, ".env");
if (existsSync(envPath)) {
  const expanded = expandedKeys(readFileSync(envPath, "utf8"), process.env);
  line(
    expanded.length === 0 || "warn",
    ".env values read as written",
    expanded.length ? `${expanded.join(", ")}: Bun expands the $ in these; use values without $` : "",
  );
}

console.log("Tools");
for (const [cmd, args] of [
  ["bun"],
  ["node"],
  ["cargo"],
  ["rustc"],
  ["docker"],
  ["git"],
  ["gh"],
  ["claude"],
  ["uv"],
] as [string, string[]?][]) {
  const v = version(cmd, args);
  line(Boolean(v), cmd, v);
}
for (const [cmd, why] of [
  ["rust-analyzer", "Rust code intelligence (rust-analyzer-lsp plugin)"],
  ["typescript-language-server", "TypeScript code intelligence (Phase 0 follow-up)"],
  ["gitleaks", "local secret scanning (CI and CodeRabbit already scan)"],
  ["tesseract", "scanned PDFs (Phase 5)"],
] as const) {
  const v = version(cmd);
  line(v ? true : "warn", `${cmd} (optional)`, v || `not installed: ${why}`);
}

console.log("\nConfiguration");
line(existsSync(join(root, ".env")), ".env present", existsSync(join(root, ".env")) ? "" : "run `bun run setup`");
for (const k of ["NEWS_PG_PASSWORD", "DATABASE_URL", "NEWS_CH_PASSWORD"])
  line(Boolean(env(k)), k, env(k) ? "set" : "empty");
line(existsSync(join(root, "site/src/styles/tokens.css")), "design tokens synced into site");

console.log("\nContainers");
for (const name of ["news-postgres", "news-otel", "news-dynamodb", env("NEWS_CH_CONTAINER", "shorts-clickhouse-1")]) {
  const r = run("docker", [
    "inspect",
    "-f",
    "{{.State.Status}} {{if .State.Health}}{{.State.Health.Status}}{{end}}",
    name,
  ]);
  line(r.ok && r.out.startsWith("running"), name, r.ok ? r.out : "not found");
}

console.log("\nServices");
try {
  const sql = new SQL(env("DATABASE_URL"));
  const [{ v }] = await sql`SELECT extversion AS v FROM pg_extension WHERE extname = 'vector'`;
  line(true, "PostgreSQL connection + pgvector", `vector ${v}`);
  await sql.close();
} catch (e) {
  line(false, "PostgreSQL connection + pgvector", String(e).slice(0, 160));
}

const ch = await chQuery("SELECT currentUser(), currentDatabase(), version()");
line(ch.ok, "ClickHouse as user news", ch.ok ? ch.body.replaceAll("\t", " ") : ch.body.slice(0, 160));
const events = await chQuery("EXISTS TABLE news.events");
line(events.ok && events.body === "1", "news.events table");

try {
  const res = await fetch(env("NEWS_OTEL_HEALTH", "http://127.0.0.1:13133"));
  line(res.ok, "OTel Collector health", `HTTP ${res.status}`);
} catch (e) {
  line(false, "OTel Collector health", String(e).slice(0, 120));
}

try {
  await requireDynamoDb();
  line(true, "DynamoDB Local", dynamoConfigFromEnv(process.env).endpoint);
} catch (e) {
  line(false, "DynamoDB Local", e instanceof Error ? e.message : String(e));
}

const cli = join(root, "cli/target/debug", process.platform === "win32" ? "news.exe" : "news");
const cliOut = existsSync(cli) ? run(cli, ["version"]).out : "";
line(Boolean(cliOut), "news CLI built", cliOut || "run `cargo build` in cli/");

if (e2e) {
  console.log("\nEnd to end");
  const marker = `doctor-${Date.now()}`;
  const ins = await chQuery(
    `INSERT INTO news.events (ts, run_id, stage, event, status, attrs) VALUES (now64(3), '${marker}', 'doctor', 'e2e', 'ok', '{}')`,
  );
  const back = await chQuery(`SELECT count() FROM news.events WHERE run_id = '${marker}'`);
  line(ins.ok && back.body === "1", "event written to and read from news.events");

  // The CLI's own event (one per call), carrying NEWS_RUN_ID and NEWS_JOB_ID (Bureau §6.2).
  if (existsSync(cli)) {
    const cliRun = `${marker}-cli`;
    run(cli, ["version"], { env: { ...process.env, NEWS_RUN_ID: cliRun, NEWS_JOB_ID: "doctor-job" } });
    const cliBack = await chQuery(
      `SELECT count() FROM news.events WHERE run_id = '${cliRun}' AND job_id = 'doctor-job' AND stage = 'cli'`,
    );
    line(
      cliBack.ok && cliBack.body === "1",
      "news CLI event in news.events with run_id and job_id",
      cliBack.ok ? "" : "run `bun run clickhouse:setup` (adds 003_ticker.sql)",
    );
  } else {
    line(false, "news CLI event in news.events with run_id and job_id", "run `cargo build` in cli/");
  }

  const now = `${Date.now()}000000`;
  const otlp = {
    resourceLogs: [
      {
        resource: { attributes: [{ key: "service.name", value: { stringValue: "news-doctor" } }] },
        scopeLogs: [{ logRecords: [{ timeUnixNano: now, severityText: "INFO", body: { stringValue: marker } }] }],
      },
    ],
  };
  let sent = false;
  try {
    const res = await fetch(`${env("NEWS_OTEL_HTTP", "http://127.0.0.1:4318")}/v1/logs`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(otlp),
    });
    sent = res.ok;
  } catch {}
  let arrived = false;
  for (let i = 0; sent && i < 20 && !arrived; i++) {
    await Bun.sleep(1000);
    const r = await chQuery(`SELECT count() FROM news.otel_logs WHERE Body = '${marker}'`);
    arrived = r.ok && r.body === "1";
  }
  line(
    arrived,
    "OTLP log through the collector into news.otel_logs",
    sent ? (arrived ? "" : "sent, not arrived within 20s") : "collector refused the log",
  );

  // Signed request to DynamoDB Local: create a throwaway table, put and get an item, delete the table.
  try {
    const item = { PK: "PAGE#doctor", SK: "LATEST", title: marker };
    const client = new DynamoDbLocal(dynamoConfigFromEnv(process.env));
    const back = await withTempTable(client, async (table) => {
      await client.putItem(table, toItem(item));
      const got = await client.getItem(table, { PK: item.PK, SK: item.SK });
      return got ? fromItem(got) : undefined;
    });
    line(back?.title === marker, "DynamoDB Local signed put/get on a throwaway table");
  } catch (e) {
    line(false, "DynamoDB Local signed put/get on a throwaway table", String(e).slice(0, 160));
  }
}

console.log(failures ? `\n${failures} problem(s) found.` : "\nAll required checks passed.");
process.exit(failures ? 1 : 0);
