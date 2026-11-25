// NEWS_DATA=local: a fixture set in this isolate's memory (ADR 0010, section 2). Dev, tests and the fixture home page.
// Never reached in production: siteData() refuses it, and nothing else imports this module. Tests change the data
// between requests through localTable() and put another set in with resetLocal().
import { FakeTable } from "@news/worker-shared/dynamo/fake";
import type { Doc } from "@news/worker-shared/dynamo/table";
import { Reader } from "@news/worker-shared/read";
import type { PageRecord } from "@news/worker-shared/records";
import { indexPage, listShards, type SearchShard } from "@news/worker-shared/search";
import { fixtureBundles } from "../renderer/fixtures/bundles/vite";
import type { SiteData, SiteEnv } from "./index";

const sets = import.meta.glob<Doc[]>("../../fixtures/data/*/items.json", { import: "default", eager: true });

/** The fixture pages that have a bundle's data behind them, for a slot's figure or chart (path → bundle). */
const BUNDLE_OF: Record<string, keyof typeof fixtureBundles> = {
  "/2025/01/apple-fy2024-results": "apple-fy2024",
  "/2025/01/fed-december-2024": "charts",
};

/** The search schema, the same files the api Worker's D1 shards are migrated with. */
const migrations = import.meta.glob<string>("../../../workers/shared/migrations/search/*.sql", {
  query: "?raw",
  import: "default",
  eager: true,
});

export const FIXTURE_SETS = Object.keys(sets).map((p) => /data\/([^/]+)\//.exec(p)?.[1] ?? p);

let table = new FakeTable();
let name = "";

/** Seeds a fresh table with `set` (default `normal`). */
export function resetLocal(set = "normal"): FakeTable {
  const items = Object.entries(sets).find(([p]) => p.includes(`/${set}/`))?.[1];
  if (!items) throw new Error(`no fixture data set ${JSON.stringify(set)}; have ${FIXTURE_SETS.join(", ")}`);
  table = new FakeTable();
  table.seed(...items);
  name = set;
  return table;
}

/** The isolate's table, for tests that change data between requests. */
export const localTable = (): FakeTable => table;

/**
 * Migrates the local D1 shards (Miniflare's, bound as SEARCH_<YYYY> in the "local" wrangler environment) and indexes the
 * fixture pages into the shard of their year, so /search and the sitemaps have data. Safe to repeat: pages are upserted.
 */
async function seedShards(shards: SearchShard[], items: Doc[]): Promise<void> {
  const pages = items.filter((i) => i.type === "page" && i.SK === "LATEST") as unknown as PageRecord[];
  for (const shard of shards) {
    // Local state outlives a run (.wrangler/state), so a statement that finds its object already there is skipped.
    // One statement starts at each CREATE: a trigger body holds semicolons but no CREATE.
    for (const sql of Object.values(migrations))
      for (const statement of sql.replace(/^--.*$/gm, "").split(/;\s*(?=CREATE\b)/))
        if (statement.trim())
          await shard.db
            .prepare(statement.trim())
            .run()
            .catch((e: unknown) => {
              if (!/already exists/.test(String(e))) throw e;
            });
    for (const page of pages) if (page.published_at.startsWith(String(shard.year))) await indexPage(shard.db, page);
  }
}

export async function localSiteData(env: SiteEnv): Promise<SiteData> {
  const set = env.NEWS_FIXTURE_SET ?? "normal";
  if (name !== set) resetLocal(set);
  const shards = listShards(env);
  await seedShards(shards, sets[Object.keys(sets).find((p) => p.includes(`/${set}/`)) ?? ""] ?? []);
  const bundle = (path: string) => {
    const b = BUNDLE_OF[path];
    return b ? fixtureBundles[b] : undefined;
  };
  return {
    // One Reader over whatever table the isolate holds now, so a test's resetLocal() is seen at once.
    get reader() {
      return new Reader(table);
    },
    curation: () => new Reader(table).getLiveCuration(),
    shards,
    async pageJson(path, file) {
      const b = bundle(path);
      return file === "data.json" ? b?.data : file === "sources.json" ? b?.sources : undefined;
    },
    async pageText(path, file) {
      return file === "page.oui" ? bundle(path)?.oui : undefined;
    },
  };
}
