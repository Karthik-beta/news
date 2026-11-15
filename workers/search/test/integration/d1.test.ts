// D1 inside workerd (Miniflare's local D1) for the search Worker: migrations apply, and FTS5 (which the Technical
// Design's search relies on) is available. The FTS table here is a throwaway probe, not the production search schema.
import { applyD1Migrations } from "cloudflare:test";
import { env } from "cloudflare:workers";
import { afterAll, beforeAll, describe, expect, it } from "vitest";

const db = () => env.SEARCH_DB;

describe("D1 migrations", () => {
  beforeAll(() => applyD1Migrations(db(), env.TEST_MIGRATIONS));

  it("apply, are recorded, and are skipped when applied again", async () => {
    const check = async () =>
      (await db().prepare("SELECT name FROM d1_migrations ORDER BY id").all<{ name: string }>()).results.map(
        (r) => r.name,
      );
    expect(await check()).toEqual(["0001_harness_check.sql"]);
    await applyD1Migrations(db(), env.TEST_MIGRATIONS);
    expect(await check()).toEqual(["0001_harness_check.sql"]);
  });

  it("created a table that accepts and enforces data", async () => {
    await db().prepare("INSERT INTO harness_check (label) VALUES (?)").bind("search-probe").run();
    const row = await db().prepare("SELECT label FROM harness_check WHERE label = ?").bind("search-probe").first();
    expect(row).toEqual({ label: "search-probe" });
    await expect(
      db().prepare("INSERT INTO harness_check (label) VALUES (?)").bind("search-probe").run(),
    ).rejects.toThrow(/UNIQUE/i);
  });
});

describe("FTS5 in D1", () => {
  const search = async (query: string) =>
    (
      await db()
        .prepare("SELECT title FROM fts_probe WHERE fts_probe MATCH ? ORDER BY title")
        .bind(query)
        .all<{ title: string }>()
    ).results.map((r) => r.title);

  beforeAll(async () => {
    await db().exec("CREATE VIRTUAL TABLE fts_probe USING fts5(title, summary, tokenize = 'porter unicode61')");
    const insert = db().prepare("INSERT INTO fts_probe (title, summary) VALUES (?, ?)");
    await db().batch([
      insert.bind("Nvidia third quarter results", "Data center revenue rose sharply on AI demand"),
      insert.bind("Federal Reserve holds rates", "The committee kept the policy rate unchanged"),
      insert.bind("Quarterly earnings roundup", "Several companies reported results this quarter"),
      insert.bind("Café chains and inflation", "Prices of coffee drinks, 5% higher"),
    ]);
  });
  afterAll(async () => {
    await db().exec("DROP TABLE fts_probe");
  });

  it("finds rows by a single word, in any column", async () => {
    expect(await search("nvidia")).toEqual(["Nvidia third quarter results"]);
    expect(await search("committee")).toEqual(["Federal Reserve holds rates"]);
  });

  it("is case-insensitive and stems words (porter): results matches result", async () => {
    expect(await search("RESULT")).toEqual(["Nvidia third quarter results", "Quarterly earnings roundup"]);
  });

  it("supports prefix, phrase, AND and NOT queries", async () => {
    expect(await search("quart*")).toEqual(["Nvidia third quarter results", "Quarterly earnings roundup"]);
    expect(await search('"third quarter"')).toEqual(["Nvidia third quarter results"]);
    expect(await search("results AND nvidia")).toEqual(["Nvidia third quarter results"]);
    expect(await search("results NOT nvidia")).toEqual(["Quarterly earnings roundup"]);
  });

  it("folds accents (unicode61): cafe finds Café", async () => {
    expect(await search("cafe")).toEqual(["Café chains and inflation"]);
  });

  it("returns an empty list when nothing matches", async () => {
    expect(await search("zeppelin")).toEqual([]);
  });

  // The search Worker must never pass raw user input to MATCH: these are normal things a reader may type.
  it.each(['"unterminated', "AND", "a OR", "news:", "(", "5% higher", "co-op"])(
    "raw user input %j is an FTS5 syntax error, so the Worker has to sanitise queries",
    async (query) => {
      await expect(search(query)).rejects.toThrow(/D1_ERROR|SQLITE_ERROR/);
    },
  );

  it("quoting each term makes the same inputs safe", async () => {
    const quote = (q: string) =>
      q
        .split(/\s+/)
        .filter(Boolean)
        .map((t) => `"${t.replaceAll('"', '""')}"`)
        .join(" ");
    for (const query of ['"unterminated', "AND", "a OR", "news:", "(", "5% higher", "co-op"]) {
      await expect(search(quote(query))).resolves.toBeInstanceOf(Array);
    }
    expect(await search(quote("5% higher"))).toEqual(["Café chains and inflation"]);
  });
});
