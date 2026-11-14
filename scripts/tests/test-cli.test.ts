import { describe, expect, test } from "bun:test";
import { createServer } from "node:net";
import { canConnect, cargoArgs, hostPort, levels, preflight, testDatabaseUrl } from "../test-cli";

describe("cargoArgs", () => {
  test("every level is locked to Cargo.lock", () => {
    for (const level of levels) expect(cargoArgs(level)).toContain("--locked");
  });

  test("unit runs in-crate tests only: no integration-test targets", () => {
    const args = cargoArgs("unit");
    expect(args).toEqual(["test", "--locked", "--workspace", "--lib", "--bins"]);
    expect(args).not.toContain("--ignored");
  });

  test("e2e runs the binary tests and not the Postgres ones", () => {
    const args = cargoArgs("e2e");
    expect(args).toContain("cli");
    expect(args).not.toContain("postgres");
    expect(args).not.toContain("--ignored");
  });

  test("integration runs only the Postgres target, including its #[ignore]d tests", () => {
    const args = cargoArgs("integration");
    expect(args.slice(args.indexOf("--test"), args.indexOf("--test") + 2)).toEqual(["--test", "postgres"]);
    expect(args.slice(-2)).toEqual(["--", "--ignored"]);
  });
});

describe("testDatabaseUrl", () => {
  test("prefers NEWS_TEST_DATABASE_URL over DATABASE_URL", () => {
    expect(testDatabaseUrl({ NEWS_TEST_DATABASE_URL: "postgres://a", DATABASE_URL: "postgres://b" })).toBe(
      "postgres://a",
    );
  });

  test("falls back to DATABASE_URL, and treats empty values as unset", () => {
    expect(testDatabaseUrl({ DATABASE_URL: "postgres://b" })).toBe("postgres://b");
    expect(testDatabaseUrl({ NEWS_TEST_DATABASE_URL: "", DATABASE_URL: "postgres://b" })).toBe("postgres://b");
    expect(testDatabaseUrl({ NEWS_TEST_DATABASE_URL: "", DATABASE_URL: "" })).toBeUndefined();
    expect(testDatabaseUrl({})).toBeUndefined();
  });
});

describe("hostPort", () => {
  test("parses host and port, defaulting to 5432", () => {
    expect(hostPort("postgres://news:pw@127.0.0.1:5433/news")).toEqual({ host: "127.0.0.1", port: 5433 });
    expect(hostPort("postgresql://u@db.internal/news")).toEqual({ host: "db.internal", port: 5432 });
  });

  test("a ?port= query parameter wins over the authority port, as the Rust driver does", () => {
    expect(hostPort("postgres://u:p@127.0.0.1/db?port=5433")).toEqual({ host: "127.0.0.1", port: 5433 });
    expect(hostPort("postgres://u:p@127.0.0.1:6000/db?port=5433")).toEqual({ host: "127.0.0.1", port: 5433 });
  });

  test("handles IPv6 literals", () => {
    expect(hostPort("postgres://u:p@[::1]:6000/db")).toEqual({ host: "::1", port: 6000 });
  });

  test("rejects other schemes, garbage and URLs without a host", () => {
    for (const bad of ["", "not a url", "http://127.0.0.1:5433/news", "mysql://u:p@h/db", "postgres:///db"])
      expect(hostPort(bad)).toBeUndefined();
  });

  test("never includes the password in its result", () => {
    expect(JSON.stringify(hostPort("postgres://news:topsecret@127.0.0.1:5433/news"))).not.toContain("topsecret");
  });
});

describe("preflight", () => {
  const up = async () => true;
  const down = async () => false;

  test("returns the URL when Postgres is reachable", async () => {
    expect(await preflight({ DATABASE_URL: "postgres://news:pw@127.0.0.1:5433/news" }, up)).toEqual({
      url: "postgres://news:pw@127.0.0.1:5433/news",
    });
  });

  test("checks the host and port taken from the URL", async () => {
    const seen: [string, number][] = [];
    await preflight({ NEWS_TEST_DATABASE_URL: "postgres://u:p@db.test:6543/x" }, async (h, p) => {
      seen.push([h, p]);
      return true;
    });
    expect(seen).toEqual([["db.test", 6543]]);
  });

  test("an unreachable database is one line telling the user to run infra:up, without the password", async () => {
    const r = await preflight({ DATABASE_URL: "postgres://news:topsecret@127.0.0.1:5433/news" }, down);
    expect(r).toEqual({ problem: "Postgres is not reachable at 127.0.0.1:5433: run `bun run infra:up`." });
    expect("problem" in r && r.problem).not.toContain("topsecret");
    expect("problem" in r && r.problem.includes("\n")).toBe(false);
  });

  test("a missing URL points at setup, and does not try to connect", async () => {
    let tried = false;
    const r = await preflight({}, async () => {
      tried = true;
      return true;
    });
    expect("problem" in r && r.problem).toContain("bun run setup");
    expect(tried).toBe(false);
  });

  test("an invalid URL is rejected without echoing its value", async () => {
    const r = await preflight({ DATABASE_URL: "definitely not a url with secret-token" }, up);
    expect("problem" in r && r.problem).toContain("not a valid postgres:// URL");
    expect("problem" in r && r.problem).not.toContain("secret-token");
  });
});

describe("canConnect (loopback sockets only)", () => {
  test("true for a listening port, false once it is closed", async () => {
    const server = createServer();
    await new Promise<void>((r) => server.listen(0, "127.0.0.1", r));
    const port = (server.address() as { port: number }).port;
    expect(await canConnect("127.0.0.1", port)).toBe(true);
    await new Promise<void>((r) => server.close(() => r()));
    expect(await canConnect("127.0.0.1", port)).toBe(false);
  });
});
