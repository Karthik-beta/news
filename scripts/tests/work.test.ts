import { describe, expect, test } from "bun:test";
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { root } from "../lib";
import { main, workArgs, workBinary, workEnv } from "../work";
import { tempDir } from "./helpers";

// A .env with the secrets the worker must never see, next to the settings it needs.
const DOTENV = [
  "DATABASE_URL=postgres://news:pg-pw@127.0.0.1:5433/news",
  "NEWS_PG_PASSWORD=pg-pw",
  "NEWS_CH_HTTP=http://127.0.0.1:8123",
  "NEWS_CH_PASSWORD=ch-pw",
  "NEWS_OTEL_HTTP=http://127.0.0.1:4318",
  "NEWS_WRITE_API_KEY=sentinel-write-key",
  "AWS_ACCESS_KEY_ID=AKIASENTINEL",
  "AWS_SECRET_ACCESS_KEY=sentinel-aws-secret",
  "CLOUDFLARE_API_TOKEN=sentinel-cf-token",
  "NEWS_DYNAMODB_SECRET_ACCESS_KEY=sentinel-dynamo",
  "",
].join("\n");
const SECRETS = ["sentinel-write-key", "sentinel-aws-secret", "sentinel-cf-token", "sentinel-dynamo", "AKIASENTINEL"];

describe("workEnv", () => {
  test("keeps the database and telemetry settings and drops every secret", () => {
    const env = workEnv(DOTENV, { Path: "C:\\bin", SystemRoot: "C:\\Windows", GITHUB_TOKEN: "sentinel-gh" });
    expect(Object.keys(env).sort()).toEqual([
      "DATABASE_URL",
      "NEWS_CH_HTTP",
      "NEWS_CH_PASSWORD",
      "NEWS_OTEL_HTTP",
      "Path",
      "SystemRoot",
    ]);
    const values = Object.values(env).join(" ");
    for (const s of [...SECRETS, "sentinel-gh"]) expect(values).not.toContain(s);
    expect(env).not.toHaveProperty("NEWS_WRITE_API_KEY");
    expect(env).not.toHaveProperty("NEWS_PG_PASSWORD");
  });

  test("an exported shell value wins over .env, and a secret exported in the shell is dropped too", () => {
    const env = workEnv(DOTENV, { NEWS_DB_SCHEMA: "news_test", NEWS_WRITE_API_KEY: "sentinel-shell-key" });
    expect(env.NEWS_DB_SCHEMA).toBe("news_test");
    expect(env).not.toHaveProperty("NEWS_WRITE_API_KEY");
  });
});

describe("main", () => {
  test("starts the binary setup builds with only the allow-listed environment", () => {
    const dir = tempDir();
    const dotenv = join(dir, ".env");
    writeFileSync(dotenv, DOTENV);
    const bin = join(dir, "news.exe");
    writeFileSync(bin, "");
    const calls: { cmd: string; args: string[]; env: Record<string, string> }[] = [];
    const code = main(
      ["--once"],
      { NEWS_WORK_BIN: bin, PATH: "/usr/bin", NEWS_WRITE_API_KEY: "sentinel-shell-key" },
      (cmd, args, env) => {
        calls.push({ cmd, args, env });
        return 0;
      },
      dotenv,
    );
    expect(code).toBe(0);
    expect(calls).toHaveLength(1);
    expect(calls[0]?.cmd).toBe(bin);
    expect(calls[0]?.args).toEqual(["work", "--pools", "net,cpu,docling", "--once"]);
    const passed = JSON.stringify(calls[0]?.env);
    for (const s of [...SECRETS, "sentinel-shell-key", "NEWS_WORK_BIN"]) expect(passed).not.toContain(s);
    expect(calls[0]?.env.DATABASE_URL).toStartWith("postgres://");
  });

  test("a missing binary is a clear error, not a spawn failure", () => {
    const code = main([], { NEWS_WORK_BIN: join(tempDir(), "nope") }, () => 0, join(tempDir(), ".env"));
    expect(code).toBe(1);
  });
});

test("the default binary is the debug build that `bun run setup` makes", () => {
  expect(workBinary({}, "win32").replaceAll("\\", "/")).toEndWith("cli/target/debug/news.exe");
  expect(workBinary({}, "linux").replaceAll("\\", "/")).toEndWith("cli/target/debug/news");
});

test("work arguments default to the code-only pools and keep an explicit --pools", () => {
  expect(workArgs([])).toEqual(["work", "--pools", "net,cpu,docling"]);
  expect(workArgs(["--pools", "net"])).toEqual(["work", "--pools", "net"]);
});

describe("infra/windows/news-work.task.xml", () => {
  test("its comments are valid XML (no -- inside a comment), so Task Scheduler can import it", () => {
    const xml = readFileSync(join(root, "infra/windows/news-work.task.xml"), "utf8");
    const comments = [...xml.matchAll(/<!--([\s\S]*?)-->/g)].map((m) => m[1] as string);
    expect(comments.length).toBeGreaterThan(0);
    for (const c of comments) expect(c).not.toContain("--");
  });
});
