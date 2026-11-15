// D1 inside workerd (Miniflare's local D1): migrations apply, queries run, errors surface.
import { applyD1Migrations } from "cloudflare:test";
import { env } from "cloudflare:workers";
import { beforeAll, describe, expect, it } from "vitest";

const db = () => env.SEARCH_DB;
const label = () => `label-${crypto.randomUUID()}`;
const insert = (value: string | null) => db().prepare("INSERT INTO harness_check (label) VALUES (?)").bind(value);
const count = async (value: string) =>
  (await db().prepare("SELECT COUNT(*) AS n FROM harness_check WHERE label = ?").bind(value).first<{ n: number }>())?.n;

describe("D1 migrations", () => {
  beforeAll(() => applyD1Migrations(db(), env.TEST_MIGRATIONS));

  it("were read from the harness migration folder", () => {
    expect(env.TEST_MIGRATIONS.map((m) => m.name)).toEqual(["0001_harness_check.sql"]);
    expect(env.TEST_MIGRATIONS[0]?.queries.length).toBeGreaterThan(0);
  });

  it("created the table and recorded the migration", async () => {
    const table = await db()
      .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'harness_check'")
      .first();
    expect(table).toEqual({ name: "harness_check" });
    const applied = await db().prepare("SELECT name FROM d1_migrations ORDER BY id").all<{ name: string }>();
    expect(applied.results.map((r) => r.name)).toEqual(["0001_harness_check.sql"]);
  });

  it("applying them again is a no-op", async () => {
    await applyD1Migrations(db(), env.TEST_MIGRATIONS);
    await applyD1Migrations(db(), env.TEST_MIGRATIONS);
    const applied = await db().prepare("SELECT COUNT(*) AS n FROM d1_migrations").first<{ n: number }>();
    expect(applied?.n).toBe(1);
  });
});

describe("D1 queries", () => {
  beforeAll(() => applyD1Migrations(db(), env.TEST_MIGRATIONS));

  it("insert, select and delete with bound parameters", async () => {
    const value = label();
    const res = await insert(value).run();
    expect(res.success).toBe(true);
    expect(res.meta.changes).toBe(1);
    expect(res.meta.last_row_id).toBeGreaterThan(0);

    const row = await db().prepare("SELECT label FROM harness_check WHERE label = ?").bind(value).first();
    expect(row).toEqual({ label: value });

    const del = await db().prepare("DELETE FROM harness_check WHERE label = ?").bind(value).run();
    expect(del.meta.changes).toBe(1);
    expect(await count(value)).toBe(0);
  });

  it("returns null / empty results for a miss instead of throwing", async () => {
    const miss = label();
    expect(await db().prepare("SELECT * FROM harness_check WHERE label = ?").bind(miss).first()).toBeNull();
    expect((await db().prepare("SELECT * FROM harness_check WHERE label = ?").bind(miss).all()).results).toEqual([]);
  });

  it("stores hostile and unicode text literally (no injection through bound values)", async () => {
    const hostile = ["Robert'); DROP TABLE harness_check;--", "emoji \u{1F4C8} and café", "a;b'c\"d\\e", "  spaced  "];
    for (const value of hostile) await insert(value).run();
    for (const value of hostile) expect(await count(value)).toBe(1);
    const table = await db().prepare("SELECT name FROM sqlite_master WHERE name = 'harness_check'").first();
    expect(table).not.toBeNull();
  });

  it("rejects a duplicate on the UNIQUE column", async () => {
    const value = label();
    await insert(value).run();
    await expect(insert(value).run()).rejects.toThrow(/UNIQUE/i);
    expect(await count(value)).toBe(1);
  });

  it("rejects NULL in a NOT NULL column", async () => {
    await expect(insert(null).run()).rejects.toThrow(/NOT NULL/i);
  });

  it("surfaces SQL errors for a missing table and a syntax error", async () => {
    await expect(db().prepare("SELECT * FROM no_such_table").all()).rejects.toThrow(/no such table/i);
    await expect(db().prepare("SELEKT 1").all()).rejects.toThrow(/syntax error/i);
  });

  it("batch is atomic: one failing statement rolls the whole batch back", async () => {
    const good = label();
    const dup = label();
    await insert(dup).run();
    await expect(db().batch([insert(good), insert(dup)])).rejects.toThrow(/UNIQUE/i);
    expect(await count(good)).toBe(0);
  });

  it("batch applies every statement when all succeed", async () => {
    const [a, b] = [label(), label()];
    const results = await db().batch([insert(a), insert(b)]);
    expect(results.map((r) => r.meta.changes)).toEqual([1, 1]);
    expect([await count(a), await count(b)]).toEqual([1, 1]);
  });
});
