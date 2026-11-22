// NEWS_DATA=local: a fixture set in this isolate's memory (ADR 0010, section 2). Dev, tests and the fixture home page.
// Never reached in production: siteData() refuses it, and nothing else imports this module. Tests change the data
// between requests through localTable() and put another set in with resetLocal().
import { FakeTable } from "@news/worker-shared/dynamo/fake";
import type { Doc } from "@news/worker-shared/dynamo/table";
import { Reader } from "@news/worker-shared/read";
import { fixtureBundles } from "../renderer/fixtures/bundles/vite";
import type { SiteData } from "./index";

const sets = import.meta.glob<Doc[]>("../../fixtures/data/*/items.json", { import: "default", eager: true });

/** The fixture pages that have a bundle's data behind them, for a slot's figure or chart (path → bundle). */
const BUNDLE_OF: Record<string, keyof typeof fixtureBundles> = {
  "/2025/01/apple-fy2024-results": "apple-fy2024",
  "/2025/01/fed-december-2024": "charts",
};

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

export function localSiteData(set = "normal"): SiteData {
  if (name !== set) resetLocal(set);
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
    shards: [],
    async pageJson(path, file) {
      const b = bundle(path);
      return file === "data.json" ? b?.data : file === "sources.json" ? b?.sources : undefined;
    },
    async pageText(path, file) {
      return file === "page.oui" ? bundle(path)?.oui : undefined;
    },
  };
}
