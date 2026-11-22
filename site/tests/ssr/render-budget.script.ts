// Run by home-render.test.ts in a Bun process with NODE_ENV=production (React's development build is several times
// slower and is not what the Worker runs). Prints the p95 render time in ms of the pages, with the data in memory.
import { FakeTable } from "@news/worker-shared/dynamo/fake";
import type { Doc } from "@news/worker-shared/dynamo/table";
import { Reader } from "@news/worker-shared/read";
import { createElement } from "react";
import items from "../../fixtures/data/normal/items.json";
import type { SiteData } from "../../src/data";
import { readFixtureBundles } from "../../src/renderer/fixtures/bundles/fs";
import { staticMarkup } from "../../src/renderer/markup";
import { matchRoute } from "../../src/server/routes";
import { type Body, loadView } from "../../src/server/views";
import { Views } from "../../src/views/Views";

const ORIGIN = "https://news.karthik.nz";
const REGIONS = ["north-america", "europe", "india", "china", "asia", "australia", "global"];
const bundles = readFixtureBundles();
const files: Record<string, unknown> = {
  "/2025/01/apple-fy2024-results data.json": bundles["apple-fy2024"].data,
  "/2025/01/apple-fy2024-results sources.json": bundles["apple-fy2024"].sources,
  "/2025/01/fed-december-2024 data.json": bundles.charts.data,
  "/2025/01/fed-december-2024 sources.json": bundles.charts.sources,
  "/2025/01/fed-december-2024 page.oui": bundles.charts.oui,
};
const table = new FakeTable();
table.seed(...(items as unknown as Doc[]));
const reader = new Reader(table);
const data: SiteData = {
  reader,
  curation: () => reader.getLiveCuration(),
  shards: [],
  pageJson: async (p, f) => files[`${p} ${f}`],
  pageText: async (p, f) => files[`${p} ${f}`] as string | undefined,
};

async function body(path: string): Promise<Body> {
  const m = matchRoute(new URL(`${ORIGIN}${path}`), ORIGIN, REGIONS);
  if (m.type !== "page") throw new Error("not a page");
  const taxonomy = await reader.listTaxonomy();
  const v = await loadView(
    { data, regions: REGIONS, now: new Date("2025-01-15T16:00:00Z"), taxonomy: async () => taxonomy },
    m,
  );
  if (v.type !== "view") throw new Error("redirect");
  return v.body;
}

function p95(b: Body): number {
  const render = () => staticMarkup(createElement(Views, { body: b }));
  for (let i = 0; i < 30; i++) render();
  const times: number[] = [];
  for (let i = 0; i < 100; i++) {
    const t = performance.now();
    render();
    times.push(performance.now() - t);
  }
  return times.sort((a, b) => a - b)[95] as number;
}

const home = await body("/");
const hub = await body("/economy");
const withFigures = p95(home);
// The same page without the figures and charts its slots read: the template and the stories' text.
if (home.type === "home") {
  if (home.model.lead) home.model.lead.figure = undefined;
  for (const s of home.model.side) s.chart = undefined;
}
console.log(
  JSON.stringify({
    hub: p95(hub),
    home: p95(home),
    homeWithFigures: withFigures,
    bytes: staticMarkup(createElement(Views, { body: hub })).length,
  }),
);
