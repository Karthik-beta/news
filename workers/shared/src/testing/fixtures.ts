// Test fixtures shared by the shared, api and mcp test suites: valid page bundles, page records and curations.
// Figures in fixture data are made-up test values, never presented as real data.
import { sha256Hex } from "../crypto";
import { hubsForPage, pageLatestKey } from "../keys";
import type { Curation, Manifest, PageMeta, PageRecord } from "../records";

export function makeMeta(overrides: Partial<PageMeta> = {}): PageMeta {
  const id = overrides.id ?? "test-page";
  return {
    schema: "meta.v1",
    id,
    path: `/2026/11/${id}`,
    version: 1,
    lang: "en",
    title: `Test page ${id}`,
    summary: "A fixture page used by the Worker tests.",
    page_type: "macro",
    tier: 2,
    categories: ["economy/inflation"],
    entities: [{ id: "country:US", name: "United States", kind: "country" }],
    tags: ["cpi"],
    region: "north-america",
    published_at: "2026-11-10T12:00:00Z",
    updated_at: "2026-11-10T12:00:00Z",
    index_status: "noindex",
    renderer_version: "r1",
    data_ids: ["dp:cpi-yoy", "dp:cpi-mom"],
    chart_ids: ["cpi-trend"],
    ...overrides,
  };
}

export interface Bundle {
  manifest: Manifest;
  files: Record<string, string>;
}

/** A complete, self-consistent bundle for `meta` (hashes and sizes computed). `extra` adds or replaces files. */
export async function makeBundle(meta: PageMeta = makeMeta(), extra: Record<string, string> = {}): Promise<Bundle> {
  const files: Record<string, string> = {
    "index.html": `<!doctype html><meta name="robots" content="${meta.index_status === "index" ? "index, follow" : "noindex"}"><title>${meta.title}</title><p>${meta.summary}</p>`,
    "index.md": `# ${meta.title}\n\n${meta.summary}\n`,
    "data.json": JSON.stringify({ points: meta.data_ids.map((id) => ({ id, value: "see source" })) }),
    "sources.json": JSON.stringify({ sources: [{ id: "src:test", url: "https://example.org/" }] }),
    "meta.json": JSON.stringify(meta),
    "page.oui": 'root = Page(title="fixture")\n',
    ...Object.fromEntries(meta.chart_ids.map((c) => [`charts/${c}.csv`, "period,value\n2026-10,x\n"])),
    ...extra,
  };
  return { manifest: await manifestFor(meta, files), files };
}

export async function manifestFor(
  meta: Pick<PageMeta, "id" | "version">,
  files: Record<string, string>,
): Promise<Manifest> {
  const enc = new TextEncoder();
  const entries = await Promise.all(
    Object.entries(files).map(async ([path, text]) => ({
      path,
      sha256: await sha256Hex(text),
      bytes: enc.encode(text).length,
    })),
  );
  entries.sort((a, b) => (a.path < b.path ? -1 : 1));
  const bundle_sha256 = await sha256Hex(entries.map((e) => `${e.path} ${e.sha256}\n`).join(""));
  return { schema: "manifest.v1", page_id: meta.id, version: meta.version, bundle_sha256, files: entries };
}

/** A live page record as the write API stores it (for seeding tables directly). */
export function makePageRecord(overrides: Partial<PageMeta> = {}, extra: Partial<PageRecord> = {}): PageRecord {
  const meta = makeMeta(overrides);
  const { schema: _schema, id, ...rest } = meta;
  return {
    ...pageLatestKey(id),
    type: "page",
    ...rest,
    page_id: id,
    status: "published",
    bundle_sha256: "0".repeat(64),
    files: ["data.json", "index.html", "index.md", "meta.json", "page.oui", "sources.json"],
    hubs: hubsForPage(meta),
    written_at: meta.updated_at,
    ...extra,
  };
}

/** Pages for a valid curation: 1 lead + 2 side + 3 sections × 3 = 15 pages, spread over 5 categories and regions. */
export function curationPages(now = "2026-11-12T00:00:00Z"): PageRecord[] {
  const categories = ["economy", "technology", "science", "business", "finance"];
  const regions = ["north-america", "europe", "india", "china", "global"];
  return Array.from({ length: 15 }, (_, i) =>
    makePageRecord({
      id: `cur-page-${i}`,
      categories: [categories[i % 5] as string],
      region: regions[i % 5] as string,
      entities: [{ id: `company:c${i}`, name: `Company ${i}`, kind: "company" }],
      published_at: now,
      updated_at: now,
      tier: i === 0 ? 1 : 2,
    }),
  );
}

export function makeCuration(pageIds: string[] = curationPages().map((p) => p.page_id)): Curation {
  const ids = [...pageIds];
  const take = () => ({ page_id: ids.shift() as string });
  return {
    schema: "curation.v1",
    lang: "en",
    created_at: "2026-11-12T08:00:00Z",
    approved_at: "2026-11-12T09:00:00Z",
    curator_model: "claude-sonnet-5-5",
    slots: {
      lead: { ...take(), figure_id: "dp:cpi-yoy" },
      side: [take(), take()],
      sections: [
        { category: "economy", pages: [take(), take(), take()] },
        { category: "technology", pages: [take(), take(), take()] },
        { category: "finance", pages: [take(), take(), { ...take(), chart_id: "cpi-trend" }] },
      ],
    },
    checks: { passed: true, results: [{ rule: "breadth", ok: true }] },
  };
}
