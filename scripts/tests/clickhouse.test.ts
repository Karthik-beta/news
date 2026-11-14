import { describe, expect, mock, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { applyAccess, dockerExecArgs, PASSWORD_PLACEHOLDER, passwordProblem, renderAccessSql } from "../clickhouse";
import { root } from "../lib";
import { makeScratchRepo, removeDir, runBun, sandboxEnv } from "./helpers";

const template = readFileSync(join(root, "infra/clickhouse/001_access.sql"), "utf8");
const PW = "Pw-with_chars123";
const occurrences = (text: string, needle: string) => text.split(needle).length - 1;
const statements = (sql: string) =>
  sql
    .split("\n")
    .filter((l) => !l.trimStart().startsWith("--"))
    .join("\n");

describe("the access SQL template", () => {
  test("holds the placeholder in CREATE USER and ALTER USER, as a quoted string, and no literal password", () => {
    const sql = statements(template);
    expect(occurrences(sql, PASSWORD_PLACEHOLDER)).toBe(2);
    expect(occurrences(sql, `BY '${PASSWORD_PLACEHOLDER}'`)).toBe(2);
  });
});

describe("renderAccessSql", () => {
  test("replaces every placeholder with the password and leaves no template syntax", () => {
    const sql = renderAccessSql(template, PW);
    expect(occurrences(statements(sql), PW)).toBe(2);
    expect(sql).not.toContain("{{");
    expect(sql).toContain(`IDENTIFIED WITH sha256_password BY '${PW}'`);
    expect(sql).toContain(`ALTER USER news IDENTIFIED WITH sha256_password BY '${PW}'`);
  });

  test("changes nothing but the placeholders", () => {
    const sql = renderAccessSql(template, PW);
    expect(sql.replaceAll(PW, PASSWORD_PLACEHOLDER)).toBe(template);
  });

  test("leaves a template without a placeholder untouched", () => {
    expect(renderAccessSql("SELECT 1", PW)).toBe("SELECT 1");
  });

  test("a password that itself looks like a placeholder is not expanded twice", () => {
    expect(renderAccessSql(`BY '${PASSWORD_PLACEHOLDER}'`, "{{x}}")).toBe("BY '{{x}}'");
  });

  // BUG (reported, not fixed): replaceAll interprets `$` patterns in its replacement string, so a password
  // containing `$&`, `$$` or `` $` `` is silently altered in the SQL while .env keeps the original, and the
  // user can then not log in. Generated passwords are base64url, so only hand-set ones are affected.
  test.failing("a password containing `$&` or `$$` reaches the SQL unchanged", () => {
    expect(renderAccessSql(`BY '${PASSWORD_PLACEHOLDER}'`, "a$&b$$c")).toBe("BY 'a$&b$$c'");
  });
});

describe("passwordProblem", () => {
  test("accepts generated base64url passwords", () => {
    expect(passwordProblem("abcDEF012-_abcDEF012-_abcDEF012-_ab")).toBeUndefined();
  });

  test("rejects an empty password and points at `bun run setup`", () => {
    expect(passwordProblem("")).toContain("bun run setup");
  });

  test("rejects single quotes and backslashes, which would break out of the SQL string", () => {
    for (const bad of ["it's", "a'b", "'; DROP DATABASE news; --", "back\\slash", "ends\\"]) {
      expect(passwordProblem(bad)).toContain("quotes or backslashes");
    }
  });

  test("accepts other punctuation and spaces", () => {
    for (const ok of ["a b", 'say "hi"', "p@ss:w/rd?x=1&y=2", "ünï"]) expect(passwordProblem(ok)).toBeUndefined();
  });
});

describe("applyAccess: the password goes on stdin, never in argv", () => {
  const rendered = renderAccessSql(template, PW);
  const fakeRun = () =>
    mock((_cmd: string, _args: string[], _opts?: { input?: string }) => ({
      ok: true,
      status: 0,
      out: "",
      err: "",
    }));

  test("runs `docker exec -i <container> clickhouse-client --multiquery` with the SQL as input", () => {
    const runner = fakeRun();
    applyAccess("my-clickhouse", rendered, runner as never);
    expect(runner).toHaveBeenCalledTimes(1);
    const [cmd, args, opts] = runner.mock.calls[0] ?? [];
    expect(cmd).toBe("docker");
    expect(args).toEqual(["exec", "-i", "my-clickhouse", "clickhouse-client", "--multiquery"]);
    expect(opts?.input).toBe(rendered);
  });

  test("the password is in the stdin input and in no argument", () => {
    const runner = fakeRun();
    applyAccess("shorts-clickhouse-1", rendered, runner as never);
    const [, args, opts] = runner.mock.calls[0] ?? [];
    expect(opts?.input).toContain(PW);
    for (const arg of args ?? []) expect(arg).not.toContain(PW);
    expect((args ?? []).join(" ")).not.toContain("BY ");
    expect((args ?? []).join(" ")).not.toContain("--password");
  });

  test("keeps docker's stdin attached (-i), otherwise the SQL would never arrive", () => {
    expect(dockerExecArgs("c")).toContain("-i");
    expect(dockerExecArgs("c").indexOf("-i")).toBeLessThan(dockerExecArgs("c").indexOf("c"));
  });

  test("passes the runner's result through, so callers can report docker's error", () => {
    const failed = { ok: false, status: 1, out: "", err: "No such container: nope" };
    expect(applyAccess("nope", "SELECT 1", (() => failed) as never)).toBe(failed);
  });

  test("targets only the named container and never anything but `docker exec`", () => {
    const runner = fakeRun();
    applyAccess("only-this-one", rendered, runner as never);
    const [cmd, args] = runner.mock.calls[0] ?? [];
    expect(cmd).toBe("docker");
    expect(args?.[0]).toBe("exec");
    expect(args?.filter((a) => a === "only-this-one")).toHaveLength(1);
    expect(args).not.toContain("rm");
    expect(args).not.toContain("restart");
  });
});

describe("bun scripts/clickhouse-setup.ts, end to end in a scratch copy (no docker on PATH)", () => {
  const runScript = (extra: Record<string, string | undefined>) => {
    const dir = makeScratchRepo();
    try {
      return runBun(["scripts/clickhouse-setup.ts"], { cwd: dir, env: sandboxEnv(dir, extra) });
    } finally {
      removeDir(dir);
    }
  };

  test("refuses to run without a password and says how to get one", () => {
    const r = runScript({ NEWS_CH_PASSWORD: "" });
    expect(r.status).toBe(1);
    expect(r.err).toContain("NEWS_CH_PASSWORD is empty");
    expect(r.err).toContain("bun run setup");
  });

  test("refuses a password with quotes or backslashes before touching docker, and does not print it", () => {
    for (const bad of ["it's-secret", "back\\slash-secret"]) {
      const r = runScript({ NEWS_CH_PASSWORD: bad });
      expect(r.status).toBe(1);
      expect(r.err).toContain("quotes or backslashes");
      expect(r.out + r.err).not.toContain("secret");
    }
  });

  test("when docker is unavailable it fails with the container name and never prints the password", () => {
    const r = runScript({ NEWS_CH_PASSWORD: "super-secret-pw", NEWS_CH_CONTAINER: "news-test-nonexistent" });
    expect(r.status).toBe(1);
    expect(r.err).toContain("Access setup failed in news-test-nonexistent");
    expect(r.out + r.err).not.toContain("super-secret-pw");
  });

  test("defaults to the shared container name from .env.example", () => {
    const r = runScript({ NEWS_CH_PASSWORD: "pw", NEWS_CH_CONTAINER: undefined });
    expect(r.err).toContain("Access setup failed in shorts-clickhouse-1");
  });
});
