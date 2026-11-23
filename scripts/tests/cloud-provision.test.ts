// infra/cloud/provision.ts against in-memory fakes of the Cloudflare and AWS clients: no network, no credentials.
import { describe, expect, test } from "bun:test";
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import type { Aws, Cf, Route, Rule, TableInfo } from "../../infra/cloud/clients";
import { main } from "../../infra/cloud/provision";
import { IAM_USERS, loadConfig, RCU, WCU } from "../../infra/cloud/steps";
import { root } from "../lib";

const ENV = {
  CLOUDFLARE_ACCOUNT_ID: "a".repeat(32),
  CLOUDFLARE_ZONE_ID: "b".repeat(32),
  CLOUDFLARE_API_TOKEN: "test-token-not-real",
  AWS_ACCOUNT_ID: "123456789012",
  AWS_REGION: "eu-west-1",
  AWS_ACCESS_KEY_ID: "AKIDLOCAL",
  AWS_SECRET_ACCESS_KEY: "test-secret-not-real",
};

/** One account's state in memory. `writes` records every call that changes something. */
function fakeCloud() {
  const state = {
    buckets: new Set<string>(),
    domains: new Map<string, string[]>(),
    d1: new Map<string, { id: string; tables: Set<string>; migrations: string[] }>(),
    rules: new Map<string, Rule[]>(),
    routes: [] as Route[],
    tables: new Map<string, TableInfo & { ttl: boolean }>(),
    users: new Map<string, unknown>(),
    writes: [] as string[],
  };
  const write = <T>(what: string, fn: () => T): T => {
    state.writes.push(what);
    return fn();
  };
  const cf: Cf = {
    bucketExists: async (n) => state.buckets.has(n),
    createBucket: async (n) => write(`bucket ${n}`, () => void state.buckets.add(n)),
    customDomains: async (b) => state.domains.get(b) ?? [],
    addCustomDomain: async (b, d) =>
      write(`domain ${d}`, () => void state.domains.set(b, [...(state.domains.get(b) ?? []), d])),
    findD1: async (n) => state.d1.get(n)?.id,
    createD1: async (n) =>
      write(`d1 ${n}`, () => {
        state.d1.set(n, { id: `id-${n}`, tables: new Set(), migrations: [] });
        return `id-${n}`;
      }),
    d1Query: async (id, sql) => {
      const db = [...state.d1.values()].find((d) => d.id === id) as NonNullable<ReturnType<typeof state.d1.get>>;
      if (sql.startsWith("SELECT name FROM sqlite_master"))
        return db.tables.has("d1_migrations") ? [{ name: "d1_migrations" }] : [];
      if (sql.startsWith("SELECT name FROM d1_migrations")) return db.migrations.map((name) => ({ name }));
      return write(`d1 sql ${id}`, () => {
        if (sql.startsWith("CREATE TABLE IF NOT EXISTS d1_migrations")) db.tables.add("d1_migrations");
        else db.migrations.push(sql.match(/VALUES \('(.+?)'\)/)?.[1] as string);
        return [];
      });
    },
    getRules: async (p) => state.rules.get(p) ?? [],
    putRules: async (p, rules) =>
      write(
        `rules ${p}`,
        () =>
          void state.rules.set(
            p,
            rules.map((r) => ({ ...r, id: "x", version: "1" })),
          ),
      ),
    routes: async () => state.routes,
    setRoute: async (r) =>
      write(`route ${r.pattern}`, () => {
        state.routes = [
          ...state.routes.filter((x) => x.pattern !== r.pattern),
          { ...r, id: r.id ?? `r${state.routes.length}` },
        ];
      }),
  };
  const awsFake: Aws = {
    describeTable: async (n) => state.tables.get(n),
    createTable: async (n, rcu, wcu) =>
      write(
        `table ${n} ${rcu}/${wcu}`,
        () => void state.tables.set(n, { provisioned: true, rcu, wcu, keysOk: true, ttl: false }),
      ),
    setThroughput: async (n, rcu, wcu) =>
      write(
        `throughput ${rcu}/${wcu}`,
        () => void Object.assign(state.tables.get(n) as object, { provisioned: true, rcu, wcu }),
      ),
    ttlEnabled: async (n) => state.tables.get(n)?.ttl ?? false,
    enableTtl: async (n) => write("ttl", () => void Object.assign(state.tables.get(n) as object, { ttl: true })),
    userExists: async (u) => state.users.has(u),
    createUser: async (u) => write(`user ${u}`, () => void state.users.set(u, undefined)),
    getUserPolicy: async (u) => state.users.get(u),
    putUserPolicy: async (u, _p, doc) => write(`policy ${u}`, () => void state.users.set(u, doc)),
  };
  return { state, clients: { cf, aws: awsFake } };
}

async function runMain(args: string[], env: Record<string, string | undefined>, cloud = fakeCloud()) {
  const out: string[] = [];
  const code = await main(
    args,
    env,
    (l) => out.push(l),
    () => cloud.clients,
  );
  return { code, out: out.join("\n"), cloud };
}

describe("dry run", () => {
  test("prints the whole plan, needs no credentials and never builds or calls a client", async () => {
    const out: string[] = [];
    const code = await main(
      ["--dry-run"],
      {},
      (l) => out.push(l),
      () => {
        throw new Error("a client was built");
      },
    );
    const text = out.join("\n");
    expect(code).toBe(0);
    for (const needle of [
      "R2 bucket news-pages",
      "R2 custom domain news.karthik.nz",
      "D1 database news-search-2026",
      "0001_pages.sql, 0002_index_status.sql",
      "D1 database news-reports",
      `${RCU} RCU / ${WCU} WCU`,
      "IAM user news-api-publish",
      "IAM user news-mcp-read",
      "IAM user news-site-read",
      "news-cache-articles",
      "news-ratelimit-mcp",
      "news.karthik.nz/2* -> no Worker",
    ]) {
      expect(text).toContain(needle);
    }
    expect(text).not.toContain("[ok]");
  });

  test("a bad setting is refused without echoing its value", async () => {
    const { code, out } = await runMain(["--dry-run"], { ...ENV, CLOUDFLARE_ZONE_ID: "not-a-zone-secret-ish" });
    expect(code).toBe(1);
    expect(out).toContain("CLOUDFLARE_ZONE_ID is not valid");
    expect(out).not.toContain("not-a-zone-secret-ish");
  });
});

describe("check and apply", () => {
  test("check on a fresh account lists every step as create and writes nothing", async () => {
    const { code, out, cloud } = await runMain(["--check"], ENV);
    expect(code).toBe(0);
    expect(out).not.toContain("[ok]");
    expect(out.match(/\[create\]/g)?.length).toBe(13);
    expect(out).toContain("13 step(s) would change.");
    expect(cloud.state.writes).toEqual([]);
  });

  test("apply creates everything within the free DynamoDB budget; the second run changes nothing", async () => {
    const cloud = fakeCloud();
    const first = await runMain([], ENV, cloud);
    expect(first.code).toBe(0);
    expect(first.out.match(/\[created\]/g)?.length).toBe(13);
    expect(cloud.state.tables.get("news")).toMatchObject({ rcu: 25, wcu: 25, provisioned: true, ttl: true });
    expect(cloud.state.users.size).toBe(3);
    expect(cloud.state.routes.map((r) => r.pattern).sort()).toEqual([
      "news.karthik.nz/*",
      "news.karthik.nz/2*",
      "news.karthik.nz/mcp*",
    ]);
    // The id of each database is printed for the wrangler bindings; migrations ran once each, in order.
    expect(first.out).toContain("news-search-2026: id-news-search-2026");
    expect(cloud.state.d1.get("news-search-2026")?.migrations).toEqual(["0001_pages.sql", "0002_index_status.sql"]);
    expect(cloud.state.d1.get("news-reports")?.migrations).toEqual(["0001_reports.sql"]);

    const writesAfterFirst = cloud.state.writes.length;
    const second = await runMain([], ENV, cloud);
    expect(second.out).not.toMatch(/\[created\]|\[updated\]/);
    expect(second.out.match(/\[ok\]/g)?.length).toBe(13);
    expect(cloud.state.writes.length).toBe(writesAfterFirst);

    const check = await runMain(["--check"], ENV, cloud);
    expect(check.out).toContain("Nothing to change.");
  });

  test("drift is repaired and other rules in the zone are kept", async () => {
    const cloud = fakeCloud();
    await runMain([], ENV, cloud);
    cloud.state.tables.set("news", { provisioned: false, rcu: 0, wcu: 0, keysOk: true, ttl: true });
    cloud.state.users.set("news-api-publish", {
      Version: "2012-10-17",
      Statement: [{ Effect: "Allow", Action: "dynamodb:*", Resource: "*" }],
    });
    const foreign: Rule = { ref: "someone-else", expression: "true", action: "block" };
    cloud.state.rules.set("http_ratelimit", [foreign]);
    cloud.state.writes.length = 0;

    const r = await runMain([], ENV, cloud);
    expect(r.out.match(/\[updated\]|\[created\]/g)?.length).toBe(3);
    expect(cloud.state.writes).toContain("throughput 25/25");
    expect(cloud.state.writes).toContain("policy news-api-publish");
    expect(cloud.state.rules.get("http_ratelimit")?.map((x) => x.ref)).toEqual(["someone-else", "news-ratelimit-mcp"]);
    expect((await runMain(["--check"], ENV, cloud)).out).toContain("Nothing to change.");
  });

  test("a table with other keys is never touched", async () => {
    const cloud = fakeCloud();
    cloud.state.tables.set("news", { provisioned: true, rcu: 5, wcu: 5, keysOk: false, ttl: false });
    const r = await runMain(["--only=dynamodb"], ENV, cloud);
    expect(r.code).toBe(1);
    expect(r.out).toContain("other keys");
    expect(cloud.state.writes).toEqual([]);
  });

  test("--only runs one group and rejects an unknown one", async () => {
    const r = await runMain(["--check", "--only=r2"], ENV);
    expect(r.out.match(/\[create\]/g)?.length).toBe(2);
    expect((await runMain(["--only=nope"], ENV)).code).toBe(2);
  });
});

describe("committed files", () => {
  test("the free DynamoDB budget is not exceeded", () => {
    expect(RCU).toBeLessThanOrEqual(25);
    expect(WCU).toBeLessThanOrEqual(25);
  });

  test("IAM policies grant no wildcard action, name one table, and the public reads are limited by leading key", () => {
    for (const { file } of IAM_USERS) {
      const doc = JSON.parse(readFileSync(join(root, "infra/cloud/iam", file), "utf8")) as {
        Statement: { Action: string[]; Resource: string; Condition?: unknown }[];
      };
      for (const s of doc.Statement) {
        expect(s.Action.some((a) => a.includes("*"))).toBe(false);
        expect(s.Resource).toMatch(/^arn:aws:dynamodb:\$\{REGION\}:\$\{ACCOUNT_ID\}:table\/\$\{TABLE\}$/);
        if (file !== "news-api-publish.json") expect(s.Condition).toBeDefined();
      }
    }
  });

  test("every migration ends with a semicolon, so the tracking insert can follow it in one call", () => {
    for (const dir of ["workers/shared/migrations/search", "workers/api/migrations/reports"]) {
      for (const f of readdirSync(join(root, dir)).filter((n) => n.endsWith(".sql"))) {
        expect(
          readFileSync(join(root, dir, f), "utf8")
            .trimEnd()
            .endsWith(";"),
          f,
        ).toBe(true);
      }
    }
  });

  test("config defaults come from the ADRs and years are parsed", () => {
    const c = loadConfig({ ...ENV, NEWS_SEARCH_YEARS: "2026,2027" }, false);
    expect(c).toMatchObject({ domain: "news.karthik.nz", bucket: "news-pages", table: "news", years: [2026, 2027] });
  });
});
