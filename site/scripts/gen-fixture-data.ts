// Writes the fixture data sets the site serves in local mode (NEWS_DATA=local) and its tests use:
// site/fixtures/data/<set>/items.json, DynamoDB items exactly as the write API stores them (a test validates them
// against contracts/cloud/dynamodb-items.schema.json). Run `bun scripts/gen-fixture-data.ts` after changing this file.
// The text is made up and carries no figure: numbers on a page come from its data, never from typed words.
//   normal         a full front page: lead, 2 side stories, 4 sections, a long "Latest"
//   withdrawn-lead the lead's page is withdrawn since the curation was approved (the slot back-fills)
//   region         normal, plus more pages for India (the region view tops up from its hub)
//   empty-latest   every published page is in a slot, so "Latest" has nothing left to show
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { type Doc, toDoc } from "@news/worker-shared/dynamo/table";
import { hubSk, hubsForPage, pageLatestKey } from "@news/worker-shared/keys";
import type { Curation, CurationRecord, HubEntry, PageRecord, PageType } from "@news/worker-shared/records";

const NOW = Date.parse("2025-01-15T16:00:00.000Z");

interface Spec {
  id: string;
  category: string;
  region: string;
  type: PageType;
  tier: number;
  hoursAgo: number;
  fly: string;
  title: string;
  summary: string;
  entity?: [string, string, "company" | "country" | "institution" | "topic"];
}

const R = ["north-america", "europe", "india", "china", "asia", "australia", "global"];
const spec = (
  id: string,
  category: string,
  type: PageType,
  tier: number,
  hoursAgo: number,
  fly: string,
  title: string,
  summary: string,
  region?: string,
  entity?: Spec["entity"],
): Spec => ({
  id,
  category,
  type,
  tier,
  hoursAgo,
  fly,
  title,
  summary,
  region: region ?? (R[hoursAgo % R.length] as string),
  ...(entity ? { entity } : {}),
});

const APPLE: Spec["entity"] = ["company:apple", "Apple", "company"];
const base: Spec[] = [
  spec(
    "apple-fy2024-results",
    "finance/earnings",
    "earnings",
    1,
    30,
    "Earnings",
    "Apple reports its results for the 2024 fiscal year",
    "Services and iPhone sales set the pace for the year, the filing shows.",
    "north-america",
    APPLE,
  ),
  spec(
    "apple-services-growth",
    "finance/earnings",
    "earnings",
    2,
    52,
    "Earnings",
    "Apple's services business keeps widening its lead",
    "The filing splits the year's sales by product line and by service.",
    "north-america",
    APPLE,
  ),
  spec(
    "bank-earnings-europe",
    "finance/earnings",
    "earnings",
    2,
    20,
    "Banks",
    "European lenders open the earnings season",
    "Net interest income and provisions are the figures to watch this quarter.",
    "europe",
    ["company:eurobank", "Eurobank", "company"],
  ),
  spec(
    "insurer-results-asia",
    "finance",
    "earnings",
    3,
    70,
    "Insurance",
    "An Asian insurer reports a steady year",
    "Premiums and claims moved in line with the previous year.",
    "asia",
    ["company:asiare", "AsiaRe", "company"],
  ),
  spec(
    "market-wrap-india",
    "finance",
    "news",
    3,
    8,
    "Markets",
    "Indian shares end the week near their highs",
    "Banks and energy firms led the index higher through the week.",
    "india",
  ),
  spec(
    "bond-yields-global",
    "finance",
    "macro",
    2,
    44,
    "Bonds",
    "Government bond yields drift lower worldwide",
    "Investors weigh slower growth against the pace of rate cuts.",
    "global",
  ),
  spec(
    "fed-december-2024",
    "economy/inflation",
    "policy",
    1,
    25,
    "America's interest rates",
    "The Federal Reserve cuts its policy rate again",
    "The committee lowered the target range at its December meeting.",
    "north-america",
    ["institution:fed", "Federal Reserve", "institution"],
  ),
  spec(
    "us-cpi-december-2024",
    "economy/inflation",
    "macro",
    2,
    3,
    "America's inflation",
    "American consumer prices rose again in December",
    "The index climbed in every month of the year, the statistics office reports.",
    "north-america",
    ["country:US", "United States", "country"],
  ),
  spec(
    "euro-inflation-flash",
    "economy/inflation",
    "macro",
    2,
    36,
    "Europe's inflation",
    "Euro-area inflation edges back towards its target",
    "The flash estimate shows services prices easing.",
    "europe",
  ),
  spec(
    "china-industrial-output",
    "economy",
    "macro",
    2,
    18,
    "China's economy",
    "China's factories speed up at the end of the year",
    "Industrial output and retail sales both beat the forecasts.",
    "china",
    ["country:CN", "China", "country"],
  ),
  spec(
    "india-gdp-estimate",
    "economy",
    "macro",
    2,
    60,
    "India's economy",
    "India's economy grows briskly in the latest quarter",
    "Services and construction carried the quarter.",
    "india",
    ["country:IN", "India", "country"],
  ),
  spec(
    "australia-jobs",
    "economy",
    "macro",
    3,
    90,
    "Australia's jobs",
    "Australian unemployment holds steady",
    "Hours worked rose even as hiring slowed.",
    "australia",
  ),
  spec(
    "japan-wages",
    "economy",
    "macro",
    3,
    110,
    "Japan's wages",
    "Japanese wages rise for a third year",
    "Bonus payments lifted the average.",
    "asia",
  ),
  spec(
    "chipmaker-capacity",
    "tech",
    "news",
    2,
    5,
    "Chips",
    "A chipmaker adds capacity for artificial intelligence",
    "The new plant will make the processors used in data centres.",
    "asia",
    ["company:chipco", "ChipCo", "company"],
  ),
  spec(
    "cloud-pricing-europe",
    "tech",
    "news",
    3,
    28,
    "Cloud",
    "Cloud providers change their pricing in Europe",
    "Egress fees fall for customers who move their data.",
    "europe",
  ),
  spec(
    "open-model-release",
    "tech",
    "report",
    2,
    40,
    "Artificial intelligence",
    "A new open model matches larger rivals on benchmarks",
    "The authors publish the weights and the evaluation code.",
    "north-america",
  ),
  spec(
    "phone-market-india",
    "tech",
    "report",
    3,
    66,
    "Phones",
    "India becomes a bigger market for premium phones",
    "Shipments of higher-priced models grew faster than the rest.",
    "india",
  ),
  spec(
    "quantum-error-correction",
    "tech",
    "science",
    3,
    96,
    "Quantum computing",
    "Researchers cut the error rate of a quantum chip",
    "The group reports a result on error correction in a preprint.",
    "global",
  ),
  spec(
    "retail-sales-quarter",
    "business",
    "earnings",
    2,
    12,
    "Retail",
    "A big retailer reports a stronger holiday quarter",
    "Online orders grew faster than store sales.",
    "north-america",
    ["company:retailco", "RetailCo", "company"],
  ),
  spec(
    "carmaker-outlook",
    "business",
    "earnings",
    2,
    38,
    "Cars",
    "A carmaker lifts its outlook for electric vehicles",
    "Margins improved as battery costs fell.",
    "europe",
    ["company:carco", "CarCo", "company"],
  ),
  spec(
    "airline-demand",
    "business",
    "news",
    3,
    54,
    "Airlines",
    "Airlines expect another busy year of travel",
    "Long-haul routes recover fastest.",
    "global",
  ),
  spec(
    "steel-merger",
    "business",
    "news",
    3,
    78,
    "Industry",
    "Two steelmakers agree to merge",
    "The deal still needs approval from regulators.",
    "china",
  ),
  spec(
    "miner-copper",
    "business",
    "report",
    3,
    102,
    "Mining",
    "A miner reports record copper output",
    "Demand from grid projects supports the price.",
    "australia",
  ),
  spec(
    "exoplanet-atmosphere",
    "science",
    "science",
    2,
    10,
    "Space",
    "A telescope reads the air of a distant planet",
    "The spectrum shows water vapour and carbon dioxide.",
    "global",
  ),
  spec(
    "walking-trial",
    "science",
    "science",
    2,
    34,
    "Exercise and blood pressure",
    "Daily walking lowered blood pressure in a trial",
    "Adults in the walking group had lower readings after a year.",
    "europe",
  ),
  spec(
    "monsoon-forecast",
    "science",
    "report",
    3,
    58,
    "Climate",
    "Forecasters expect a stronger monsoon",
    "Ocean temperatures in the Pacific are the main factor.",
    "india",
  ),
  spec(
    "fusion-record",
    "science",
    "science",
    2,
    84,
    "Energy",
    "A fusion experiment sets a duration record",
    "The plasma was held for longer than in any earlier run.",
    "china",
  ),
  spec(
    "coral-bleaching",
    "science",
    "report",
    3,
    106,
    "Oceans",
    "Coral bleaching spreads on the Great Barrier Reef",
    "Surveys found damage across most of the reef's regions.",
    "australia",
  ),
  spec(
    "rates-explainer",
    "economy",
    "explainer",
    3,
    120,
    "Explainer",
    "How central banks set interest rates",
    "A plain guide to the tools a central bank uses.",
    "global",
  ),
];

const extraIndia: Spec[] = Array.from({ length: 9 }, (_, i) =>
  spec(
    `india-extra-${i + 1}`,
    ["economy", "tech", "business", "finance", "science", "economy", "tech", "business", "finance"][i] as string,
    "news",
    3,
    130 + i * 6,
    [
      "India's economy",
      "India's tech",
      "India's business",
      "India's markets",
      "India's science",
      "India's trade",
      "India's chips",
      "India's retail",
      "India's banks",
    ][i] as string,
    [
      "Trade data shows a wider surplus in services",
      "A start-up opens a design centre in Bengaluru",
      "A conglomerate reports its quarter",
      "Mutual funds see steady inflows",
      "A mission sends back its first images",
      "Exports of electronics keep rising",
      "A fab plans a pilot line",
      "A chain opens more small stores",
      "Lenders report lower bad loans",
    ][i] as string,
    "A fixture page used to fill the India region view.",
    "india",
  ),
);

const NAMES: Record<string, string> = {
  tech: "Tech",
  business: "Business",
  finance: "Finance",
  "finance/earnings": "Earnings",
  economy: "Economy",
  "economy/inflation": "Inflation",
  science: "Science",
};

const iso = (hoursAgo: number) => new Date(NOW - hoursAgo * 3_600_000).toISOString();

function record(s: Spec, withdrawn: boolean): { page: PageRecord; entries: HubEntry[] } {
  const published = iso(s.hoursAgo);
  const day = published.slice(0, 7).split("-");
  const meta = {
    categories: [s.category],
    entities: s.entity ? [{ id: s.entity[0], name: s.entity[1], kind: s.entity[2] }] : [],
    region: s.region,
  };
  const hubs = hubsForPage(meta);
  const page: PageRecord = {
    ...pageLatestKey(s.id),
    type: "page",
    lang: "en",
    page_id: s.id,
    path: `/${day[0]}/${day[1]}/${s.id}`,
    version: 1,
    status: withdrawn ? "withdrawn" : "published",
    index_status: "index",
    title: s.title,
    fly_title: s.fly,
    summary: s.summary,
    page_type: s.type,
    tier: s.tier,
    categories: meta.categories,
    entities: meta.entities,
    tags: [],
    region: s.region,
    published_at: published,
    updated_at: published,
    renderer_version: "r1",
    data_ids:
      s.id === "apple-fy2024-results"
        ? ["dp_aapl_net_sales_fy2024"]
        : s.id === "fed-december-2024"
          ? ["dp_fed_upper_20241218"]
          : [],
    chart_ids: s.id === "fed-december-2024" ? ["path"] : [],
    bundle_sha256: "0".repeat(64),
    files: ["data.json", "index.html", "index.md", "meta.json", "page.oui", "sources.json"],
    hubs: withdrawn ? [] : hubs,
    written_at: published,
    ...(withdrawn ? { withdrawn_at: iso(1), withdraw_reason: "fixture: withdrawn after the curation" } : {}),
  };
  const entries: HubEntry[] = withdrawn
    ? []
    : hubs.map((hub) => ({
        PK: `HUB#${hub}`,
        SK: hubSk(published, s.id),
        type: "hub_entry",
        lang: "en",
        hub,
        page_id: s.id,
        path: page.path,
        title: s.title,
        fly_title: s.fly,
        summary: s.summary,
        page_type: s.type,
        tier: s.tier,
        region: s.region,
        published_at: published,
        updated_at: published,
        index_status: "index",
      }));
  return { page, entries };
}

const curation = (): Curation => ({
  schema: "curation.v1",
  lang: "en",
  created_at: iso(14),
  approved_at: iso(13),
  curator_model: "fixture",
  slots: {
    lead: { page_id: "apple-fy2024-results", figure_id: "dp_aapl_net_sales_fy2024" },
    side: [{ page_id: "fed-december-2024", chart_id: "path" }, { page_id: "chipmaker-capacity" }],
    sections: [
      {
        category: "economy",
        pages: ["us-cpi-december-2024", "china-industrial-output", "india-gdp-estimate"].map((page_id) => ({
          page_id,
        })),
      },
      {
        category: "business",
        pages: ["retail-sales-quarter", "carmaker-outlook", "airline-demand"].map((page_id) => ({ page_id })),
      },
      {
        category: "science",
        pages: ["exoplanet-atmosphere", "walking-trial", "monsoon-forecast"].map((page_id) => ({ page_id })),
      },
      {
        category: "tech",
        pages: ["open-model-release", "cloud-pricing-europe", "phone-market-india"].map((page_id) => ({ page_id })),
      },
    ],
  },
  checks: { passed: true, results: [{ rule: "breadth", ok: true }] },
});

const slotIds = (c: Curation) => [
  c.slots.lead.page_id,
  ...c.slots.side.map((s) => s.page_id),
  ...c.slots.sections.flatMap((s) => s.pages.map((p) => p.page_id)),
];

function build(specs: Spec[], opts: { withdrawn?: string[] } = {}): Doc[] {
  const c = curation();
  const items: Doc[] = [];
  const entities = new Map<string, Spec["entity"]>();
  for (const s of specs) {
    const { page, entries } = record(s, opts.withdrawn?.includes(s.id) ?? false);
    items.push(toDoc(page), ...entries.map((e) => toDoc(e)));
    if (s.entity) entities.set(s.entity[0], s.entity);
  }
  for (const e of entities.values())
    items.push(
      toDoc({
        PK: `ENT#${e?.[0]}`,
        SK: "META",
        type: "entity",
        lang: "en",
        id: e?.[0],
        name: e?.[1],
        kind: e?.[2],
        updated_at: iso(200),
      }),
    );
  for (const [order, [id, name]] of Object.entries(NAMES).entries())
    items.push(
      toDoc({
        PK: "TAX#tree",
        SK: `NODE#${id}`,
        type: "taxonomy_node",
        lang: "en",
        id,
        name,
        parent: id.includes("/") ? id.split("/")[0] : null,
        order,
        updated_at: iso(200),
      }),
    );
  const record1: CurationRecord = {
    ...c,
    PK: "HOME#CURATION",
    SK: "LATEST",
    type: "curation",
    version: 1,
    published_at: iso(12),
    made_live_at: iso(12),
    server_checks: { passed: true, run_at: iso(12) },
  };
  items.push(toDoc(record1));
  return items;
}

const curated = new Set(slotIds(curation()));
const sets: Record<string, Doc[]> = {
  normal: build(base),
  "withdrawn-lead": build(base, { withdrawn: ["apple-fy2024-results"] }),
  region: build([...base, ...extraIndia]),
  "empty-latest": build(base.filter((s) => curated.has(s.id))),
};

const out = join(import.meta.dir, "..", "fixtures", "data");
for (const [name, items] of Object.entries(sets)) {
  mkdirSync(join(out, name), { recursive: true });
  writeFileSync(join(out, name, "items.json"), `[\n${items.map((i) => JSON.stringify(i)).join(",\n")}\n]\n`);
  console.log(`${name}: ${items.length} items`);
}
