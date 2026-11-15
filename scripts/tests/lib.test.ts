import { afterEach, describe, expect, spyOn, test } from "bun:test";
import { existsSync } from "node:fs";
import { join, resolve } from "node:path";
import { chQuery, env, mark, root, run } from "../lib";
import { withEnv } from "./helpers";

const bun = process.execPath;

describe("root", () => {
  test("is the repo root: holds package.json and scripts/", () => {
    expect(existsSync(join(root, "package.json"))).toBe(true);
    expect(existsSync(join(root, "scripts", "lib.ts"))).toBe(true);
  });
});

describe("env", () => {
  test("returns the variable when set", () =>
    withEnv({ NEWS_TEST_X: "value" }, () => expect(env("NEWS_TEST_X", "fallback")).toBe("value")));

  test("returns the fallback when unset, and '' without one", () =>
    withEnv({ NEWS_TEST_X: undefined }, () => {
      expect(env("NEWS_TEST_X", "fallback")).toBe("fallback");
      expect(env("NEWS_TEST_X")).toBe("");
    }));

  test("an empty variable (e.g. `KEY=` in .env) uses the fallback, and '' without one", () =>
    withEnv({ NEWS_TEST_X: "" }, () => {
      expect(env("NEWS_TEST_X", "fallback")).toBe("fallback");
      expect(env("NEWS_TEST_X")).toBe("");
    }));

  test("a whitespace-only value is kept as set (only an empty value falls back)", () =>
    withEnv({ NEWS_TEST_X: " " }, () => expect(env("NEWS_TEST_X", "fallback")).toBe(" ")));
});

describe("mark", () => {
  test("maps true, false and warn to ✓ ✗ !", () => {
    expect([mark(true), mark(false), mark("warn")]).toEqual(["✓", "✗", "!"]);
  });
});

describe("run", () => {
  test("reports success with trimmed stdout", () => {
    const r = run(bun, ["-e", "process.stdout.write('  hello \\n')"]);
    expect(r).toEqual({ ok: true, status: 0, out: "hello", err: "" });
  });

  test("captures stderr and a non-zero exit code without throwing", () => {
    const r = run(bun, ["-e", "console.error('boom'); process.exit(3)"]);
    expect(r.ok).toBe(false);
    expect(r.status).toBe(3);
    expect(r.err).toBe("boom");
    expect(r.out).toBe("");
  });

  test("a command that does not exist is a failure with no status, not an exception", () => {
    const r = run("news-no-such-command-xyz", ["--version"]);
    expect(r.ok).toBe(false);
    expect(r.status).toBeNull();
    expect(r.out).toBe("");
  });

  test("a spawn failure is reported in `err` so callers can show why (e.g. docker missing)", () => {
    const r = run("news-no-such-command-xyz", ["--version"]);
    expect(r.err).toMatch(/ENOENT|not found/i);
    expect(r.err).toContain("news-no-such-command-xyz");
  });

  test("passes `input` on stdin", () => {
    const r = run(bun, ["-e", "process.stdout.write(require('fs').readFileSync(0, 'utf8').toUpperCase())"], {
      input: "from stdin",
    });
    expect(r.out).toBe("FROM STDIN");
  });

  test("runs in the repo root by default and honours a cwd override", () => {
    const cwd = (opts = {}) => run(bun, ["-e", "process.stdout.write(process.cwd())"], opts).out;
    expect(resolve(cwd())).toBe(resolve(root));
    expect(resolve(cwd({ cwd: join(root, "scripts") }))).toBe(resolve(root, "scripts"));
  });

  test("arguments are not interpreted by a shell", () => {
    const r = run(bun, ["-e", "process.stdout.write(process.argv[1])", "a b; echo injected && c"]);
    expect(r.out).toBe("a b; echo injected && c");
  });
});

describe("chQuery", () => {
  const ENV = {
    NEWS_CH_HTTP: undefined,
    NEWS_CH_USER: undefined,
    NEWS_CH_PASSWORD: "pa:ss word",
  };
  let fetchSpy: ReturnType<typeof spyOn> | undefined;
  afterEach(() => fetchSpy?.mockRestore());

  const mockFetch = (impl: (url: unknown, init: RequestInit) => Response | Promise<Response>) => {
    fetchSpy = spyOn(globalThis, "fetch").mockImplementation(((url: unknown, init: RequestInit) =>
      Promise.resolve(impl(url, init))) as unknown as typeof fetch);
    return fetchSpy;
  };

  test("POSTs the SQL as the body to the local ClickHouse with Basic auth as user news", () =>
    withEnv(ENV, async () => {
      const spy = mockFetch(() => new Response("1\n"));
      const result = await chQuery("SELECT 1");
      expect(result).toEqual({ ok: true, body: "1" });
      const [url, init] = spy.mock.calls[0] as [string, RequestInit];
      expect(url).toBe("http://127.0.0.1:8123");
      expect(init.method).toBe("POST");
      expect(init.body).toBe("SELECT 1");
      expect((init.headers as Record<string, string>).authorization).toBe(`Basic ${btoa("news:pa:ss word")}`);
    }));

  test("never puts the password in the URL", () =>
    withEnv(ENV, async () => {
      const spy = mockFetch(() => new Response("ok"));
      await chQuery("SELECT 1");
      expect(String(spy.mock.calls[0]?.[0])).not.toContain("pa:ss");
      expect(String(spy.mock.calls[0]?.[0])).not.toContain(encodeURIComponent("pa:ss word"));
    }));

  test("uses NEWS_CH_HTTP and NEWS_CH_USER when set", () =>
    withEnv({ ...ENV, NEWS_CH_HTTP: "http://ch.example.test:9000", NEWS_CH_USER: "other" }, async () => {
      const spy = mockFetch(() => new Response(""));
      await chQuery("SELECT 1");
      const [url, init] = spy.mock.calls[0] as [string, RequestInit];
      expect(url).toBe("http://ch.example.test:9000");
      expect((init.headers as Record<string, string>).authorization).toBe(`Basic ${btoa("other:pa:ss word")}`);
    }));

  test("an HTTP error is ok:false with the server's message, trimmed", () =>
    withEnv(ENV, async () => {
      mockFetch(() => new Response("Code: 516. Authentication failed\n\n", { status: 403 }));
      expect(await chQuery("SELECT 1")).toEqual({ ok: false, body: "Code: 516. Authentication failed" });
    }));

  test("a network failure is ok:false with the error text, not a throw", () =>
    withEnv(ENV, async () => {
      fetchSpy = spyOn(globalThis, "fetch").mockRejectedValue(new Error("connect ECONNREFUSED 127.0.0.1:8123"));
      const result = await chQuery("SELECT 1");
      expect(result.ok).toBe(false);
      expect(result.body).toContain("ECONNREFUSED");
    }));

  test("a non-Latin1 password is sent as UTF-8 Basic auth, not a throw", () =>
    withEnv({ ...ENV, NEWS_CH_PASSWORD: "pässwörd-€-密码" }, async () => {
      const spy = mockFetch(() => new Response("1"));
      expect(await chQuery("SELECT 1")).toEqual({ ok: true, body: "1" });
      const [, init] = spy.mock.calls[0] as [string, RequestInit];
      const expected = Buffer.from("news:pässwörd-€-密码", "utf8").toString("base64");
      expect((init.headers as Record<string, string>).authorization).toBe(`Basic ${expected}`);
    }));

  test("empty NEWS_CH_HTTP and NEWS_CH_USER fall back to the local defaults", () =>
    withEnv({ ...ENV, NEWS_CH_HTTP: "", NEWS_CH_USER: "" }, async () => {
      const spy = mockFetch(() => new Response(""));
      await chQuery("SELECT 1");
      const [url, init] = spy.mock.calls[0] as [string, RequestInit];
      expect(url).toBe("http://127.0.0.1:8123");
      expect((init.headers as Record<string, string>).authorization).toBe(`Basic ${btoa("news:pa:ss word")}`);
    }));

  test("an empty response body is ok:true with ''", () =>
    withEnv(ENV, async () => {
      mockFetch(() => new Response(""));
      expect(await chQuery("CREATE TABLE IF NOT EXISTS t (x Int8) ENGINE = Memory")).toEqual({ ok: true, body: "" });
    }));
});
