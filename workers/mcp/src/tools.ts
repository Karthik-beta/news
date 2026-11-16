// The five read-only tools. Schemas come from contracts/cloud/mcp-tools.schema.json (one source for tools/list and
// for the precompiled validators that check every call's arguments).
import { categoryHub, entityHub } from "@news/worker-shared/keys";
import { type CappedText, pageObjectKey, readCapped } from "@news/worker-shared/r2";
import type { PageRecord } from "@news/worker-shared/records";
import { MAX_TERMS, QueryTooLongError, searchPages } from "@news/worker-shared/search";
import { absolute, hubPath } from "@news/worker-shared/urls";
import type { Validator } from "@news/worker-shared/validate";
import { schemas } from "@news/worker-shared/validate";
import contract from "../../../contracts/cloud/mcp-tools.schema.json";
import type { Deps } from "./deps";

/** Size caps (ADR 0006): file content returned by one call, and the whole serialized result. */
export const CAPS = {
  markdownBytes: 60_000,
  dataBytes: 100_000,
  resultBytes: 512 * 1024,
} as const;

/** A failure the model can act on: reported as a tool result with isError, not as a protocol error. */
export class ToolError extends Error {}

export interface ToolOutput {
  structured: Record<string, unknown>;
  /** Text for the content block. Defaults to the serialized structured result. */
  text?: string;
}

interface Tool {
  name: string;
  title: string;
  description: string;
  input: Validator;
  inputSchema: object;
  outputSchema: object;
  run(deps: Deps, args: Record<string, unknown>): Promise<ToolOutput>;
}

const defs = contract.$defs;
const READ_ONLY = { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false };

async function livePage(deps: Deps, args: { page_id?: string; path?: string }): Promise<PageRecord> {
  const reader = deps.reader();
  const id = args.page_id ?? (args.path ? await reader.getPageIdByPath(args.path) : undefined);
  const page = id ? await reader.getPage(id) : undefined;
  if (!page) throw new ToolError(`No page found for ${args.page_id ? `ID ${args.page_id}` : `path ${args.path}`}.`);
  if (page.status !== "published") throw new ToolError(`Page ${page.page_id} has been withdrawn.`);
  return page;
}

/**
 * Reads one file of the live page and makes sure it belongs to the version the page record names. Files are
 * rewritten in place before a publish moves the live record, so for a moment the stored file can be newer than the
 * record (each file carries its version as R2 metadata). Then the record is read again once; if they still differ the
 * caller is told to retry, so content is never labelled with another version.
 */
async function readLiveFile(
  deps: Deps,
  page: PageRecord,
  file: string,
  cap: number,
): Promise<{ page: PageRecord; got: CappedText }> {
  let current = page;
  for (let attempt = 0; attempt < 2; attempt++) {
    const got = await readCapped(deps.bucket(), pageObjectKey(current.path, file), cap);
    if (!got && attempt > 0) break; // the newer version dropped the file
    if (!got) throw new Error(`${file} missing in storage for ${current.page_id}`);
    if (got.version === undefined || got.version === current.version) return { page: current, got };
    current = await livePage(deps, { page_id: current.page_id });
  }
  throw new ToolError(`Page ${page.page_id} is being updated right now; try again in a few seconds.`);
}

const summaryOf = (deps: Deps, p: PageRecord) => ({
  page_id: p.page_id,
  url: absolute(deps.siteOrigin, p.path),
  version: p.version,
  lang: p.lang,
  title: p.title,
});

/** The public view of a page record: no storage keys, hashes or internal bookkeeping. */
function publicPage(deps: Deps, p: PageRecord): Record<string, unknown> {
  return {
    page_id: p.page_id,
    url: absolute(deps.siteOrigin, p.path),
    lang: p.lang,
    version: p.version,
    title: p.title,
    ...(p.fly_title ? { fly_title: p.fly_title } : {}),
    summary: p.summary,
    page_type: p.page_type,
    tier: p.tier,
    categories: p.categories,
    entities: p.entities,
    tags: p.tags,
    region: p.region,
    published_at: p.published_at,
    updated_at: p.updated_at,
    data_ids: p.data_ids,
    chart_ids: p.chart_ids,
    formats: {
      markdown: absolute(deps.siteOrigin, `${p.path}/index.md`),
      data: absolute(deps.siteOrigin, `${p.path}/data.json`),
    },
  };
}

export const TOOLS: Tool[] = [
  {
    name: "search_pages",
    title: "Search pages",
    description:
      "Keyword search over News pages (titles, summaries, entities, tags, categories). Returns up to 20 pages with title, summary and URL. Optional filters: category, region, page type and publication dates.",
    input: schemas.validateSearchPagesInput,
    inputSchema: defs.searchPagesInput,
    outputSchema: defs.searchPagesOutput,
    async run(deps, args) {
      const shards = deps.shards();
      if (shards.length === 0) throw new Error("no search shard is bound");
      let hits: Awaited<ReturnType<typeof searchPages>>;
      try {
        hits = await searchPages(shards, { ...(args as { query: string }), limit: Number(args.limit ?? 10) });
      } catch (e) {
        if (e instanceof QueryTooLongError) throw new ToolError(`Use at most ${MAX_TERMS} words; every word must match.`);
        throw e;
      }
      return {
        structured: {
          results: hits.map((h) => ({
            page_id: h.page_id,
            title: h.title,
            summary: h.summary,
            url: absolute(deps.siteOrigin, h.path),
            published_at: h.published_at,
            categories: h.categories,
            region: h.region,
            page_type: h.page_type,
            lang: h.lang,
          })),
        },
      };
    },
  },
  {
    name: "get_page",
    title: "Get a page",
    description:
      "One page by ID or path, as Markdown (the page's text with its figures, as published) or as JSON metadata. Long Markdown is truncated; the result then links to the full file.",
    input: schemas.validateGetPageInput,
    inputSchema: defs.getPageInput,
    outputSchema: defs.getPageOutput,
    async run(deps, args) {
      const record = await livePage(deps, args as { page_id?: string; path?: string });
      if (args.format === "json") {
        return {
          structured: {
            ...summaryOf(deps, record),
            format: "json",
            page: publicPage(deps, record),
            truncated: false,
          },
        };
      }
      const { page, got: md } = await readLiveFile(deps, record, "index.md", CAPS.markdownBytes);
      const base = summaryOf(deps, page);
      const note = md.truncated ? `

[Truncated. Full page: ${base.url}/index.md]` : "";
      return {
        structured: { ...base, format: "markdown", markdown: md.text, truncated: md.truncated },
        text: md.text + note,
      };
    },
  },
  {
    name: "list_categories",
    title: "List categories",
    description: "The category tree: every category and sub-category with its parent, order and hub URL.",
    input: schemas.validateListCategoriesInput,
    inputSchema: defs.listCategoriesInput,
    outputSchema: defs.listCategoriesOutput,
    async run(deps) {
      const nodes = await deps.reader().listTaxonomy();
      return {
        structured: {
          categories: nodes.slice(0, 1000).map((n) => ({
            id: n.id,
            name: n.name,
            parent: n.parent,
            order: n.order,
            lang: n.lang,
            url: absolute(deps.siteOrigin, hubPath(categoryHub(n.id))),
          })),
        },
      };
    },
  },
  {
    name: "get_entity",
    title: "Get an entity",
    description:
      "A company, person, country, institution or topic by entity ID (for example 'company:apple'), with its most recent pages.",
    input: schemas.validateGetEntityInput,
    inputSchema: defs.getEntityInput,
    outputSchema: defs.getEntityOutput,
    async run(deps, args) {
      const id = String(args.entity_id);
      const reader = deps.reader();
      const entity = await reader.getEntity(id);
      if (!entity) throw new ToolError(`No entity with ID ${id}.`);
      const hub = await reader.listHub(entityHub(id), { limit: Number(args.limit ?? 10) });
      return {
        structured: {
          entity: { id: entity.id, name: entity.name, kind: entity.kind, lang: entity.lang },
          pages: hub.items.map((e) => ({
            page_id: e.page_id,
            title: e.title,
            summary: e.summary,
            url: absolute(deps.siteOrigin, e.path),
            published_at: e.published_at,
            lang: e.lang,
          })),
        },
      };
    },
  },
  {
    name: "get_data",
    title: "Get a page's data",
    description:
      "The data behind a page (its data.json) or behind one chart (that chart's CSV), exactly as published with source IDs. Values are never converted. Large files are truncated; the result then links to the full file.",
    input: schemas.validateGetDataInput,
    inputSchema: defs.getDataInput,
    outputSchema: defs.getDataOutput,
    async run(deps, args) {
      const record = await livePage(deps, { page_id: String(args.page_id) });
      const chart = args.chart_id === undefined ? undefined : String(args.chart_id);
      const file = chart ? `charts/${chart}.csv` : "data.json";
      if (chart && !record.files.includes(file))
        throw new ToolError(`Page ${record.page_id} has no chart data for ${chart}.`);
      const { page, got } = await readLiveFile(deps, record, file, CAPS.dataBytes);
      const url = absolute(deps.siteOrigin, `${page.path}/${file}`);
      const note = got.truncated ? `\n\n[Truncated at ${CAPS.dataBytes} bytes. Full file: ${url}]` : "";
      return {
        structured: {
          page_id: page.page_id,
          ...(chart ? { chart_id: chart } : {}),
          url,
          media_type: chart ? "text/csv" : "application/json",
          content: got.text,
          truncated: got.truncated,
        },
        text: got.text + note,
      };
    },
  },
];

export const TOOL_BY_NAME = new Map(TOOLS.map((t) => [t.name, t]));

/** Tool definitions as tools/list sends them, in a fixed order. */
export const TOOL_LIST = TOOLS.map((t) => ({
  name: t.name,
  title: t.title,
  description: t.description,
  inputSchema: t.inputSchema,
  outputSchema: t.outputSchema,
  annotations: { title: t.title, ...READ_ONLY },
}));
