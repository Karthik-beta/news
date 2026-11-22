// What each data route shows: reads what the page needs (a handful of calls, ADR 0010 section 2) and returns a view
// model for the templates in src/views. The page text is always a page record's or a hub entry's; a curation names
// pages and the figure or chart to show, never words.
import { slotsOf } from "@news/worker-shared/curation";
import { keyOf } from "@news/worker-shared/dynamo/table";
import { encodeCursor, type Reader } from "@news/worker-shared/read";
import type { Curation, HubEntry, TaxonomyNode } from "@news/worker-shared/records";
import { newsLibrary } from "../components/library";
import type { NavLink } from "../components/Masthead/Masthead";
import type { SiteData } from "../data";
import { BACKFILL_DEPTH, backfillHubs, composeHome, composeRegion, type HomeModel, type Story } from "../home/compose";
import { PageData, PageSources } from "../renderer/bundle/schema";
import { type ListedPage, listingParts, type PageListing, regionLabel } from "../renderer/data/listings";
import { PageDataResolver } from "../renderer/data/resolve";
import { parsePage } from "../renderer/render";
import { canonicalPath, type RouteKind } from "./routes";
import type { PageMatch } from "./serve";

/** A page's JSON files are not read past this size: parsing costs about 8 ms of CPU per MiB (ADR 0006). */
export const MAX_DATA_BYTES = 256 * 1024;
export const HUB_PAGE = 20;
/** A region filter examines at most this many entries to find a page's worth. */
export const SCAN_LIMIT = 200;
export const LATEST_READ = 30;
export const STRIP_FIGURES = 4;

export interface Ctx {
  data: SiteData;
  regions: readonly string[];
  now: Date;
  /** The category tree, cached in the isolate for about a minute. */
  taxonomy(): Promise<TaxonomyNode[]>;
}

export interface Chrome {
  nav: NavLink[];
  /** The masthead's current section (its href). */
  current?: string;
  title: string;
  /** `index, follow` for the first page of a view, `noindex, follow` for cursor pages, filtered views and 404. */
  robots: "index, follow" | "noindex, follow";
  /** The path of the canonical URL (indexed pages only). */
  canonical?: string;
}

export interface RegionOption {
  slug: string;
  label: string;
}

export type Body =
  | {
      type: "home" | "region";
      date: string;
      now: string;
      /** "all" on the home page, the region code on a region view. */
      selected: string;
      regions: RegionOption[];
      model: HomeModel;
      /** Taxonomy names of the sections' categories. */
      names: Record<string, string>;
      /** Every page the model shows, with the data its slot's figure or chart reads. */
      pages: ListedPage[];
      /** A region view's link to the older pages of its hub. */
      olderHref?: string;
      region?: string;
    }
  | {
      type: "hub";
      kind: Exclude<RouteKind, "home" | "region">;
      title: string;
      fly?: string;
      subcategories: NavLink[];
      /** The region filter (category hubs and Latest), over this hub's path. */
      filter?: { base: string; selected: string; regions: RegionOption[] };
      entries: HubEntry[];
      olderHref?: string;
      /** Entity hubs: the entries grouped by year, newest first. */
      years?: { year: string; entries: HubEntry[] }[];
      /** Entity hubs: the newest page and its first figures. */
      strip?: { page: ListedPage; figureIds: string[] };
    }
  | { type: "notfound" };

export type View =
  | { type: "view"; chrome: Chrome; body: Body }
  | { type: "redirect"; location: string; status: 301 | 308 };

export const listedOf = (s: Story, rest: Omit<ListedPage, "record"> = {}): ListedPage => {
  const record: PageListing = {
    page_id: s.page_id,
    path: s.path,
    title: s.title,
    fly_title: s.fly_title ?? null,
    summary: s.summary,
    page_type: s.page_type,
    region: s.region,
    published_at: s.published_at,
  };
  return { record, ...rest };
};

const regionOptions = (regions: readonly string[]): RegionOption[] => [
  { slug: "all", label: "All regions" },
  ...regions.map((slug) => ({ slug, label: regionLabel(slug) })),
];

/** A page's data for a slot's figure or chart, read by code; undefined when it cannot be read (the story stays). */
async function pageData(
  data: SiteData,
  story: Story,
  wantsChart: boolean,
): Promise<Omit<ListedPage, "record"> | undefined> {
  try {
    const [d, s, oui] = await Promise.all([
      data.pageJson(story.path, "data.json", MAX_DATA_BYTES),
      data.pageJson(story.path, "sources.json", MAX_DATA_BYTES),
      wantsChart ? data.pageText(story.path, "page.oui", MAX_DATA_BYTES) : undefined,
    ]);
    const pd = PageData.safeParse(d);
    const ps = PageSources.safeParse(s);
    if (!pd.success || !ps.success) return undefined;
    const parts = oui ? listingParts(parsePage(oui, newsLibrary().library)) : {};
    return { data: new PageDataResolver(pd.data, ps.data), ...parts };
  } catch {
    return undefined;
  }
}

/** The listed pages of a model. A slot whose figure or chart cannot be read keeps its story and loses the figure. */
async function listModel(data: SiteData, model: HomeModel): Promise<ListedPage[]> {
  const slots = [model.lead, ...model.side, ...model.sections.flatMap((s) => s.slots)].flatMap((s) => s ?? []);
  const listed = await Promise.all(
    slots.map(async (slot) => {
      if (!slot.figure && !slot.chart) return listedOf(slot.story);
      const extra = await pageData(data, slot.story, Boolean(slot.chart));
      if (!extra) {
        slot.figure = undefined;
        slot.chart = undefined;
      }
      return listedOf(slot.story, extra);
    }),
  );
  return [...listed, ...model.latest.map((e) => listedOf(e))];
}

async function chrome(ctx: Ctx, over: Partial<Chrome> & Pick<Chrome, "title">): Promise<Chrome> {
  const tax = await ctx.taxonomy();
  return {
    nav: tax.filter((n) => n.parent === null).map((n) => ({ label: n.name, href: `/${n.id}` })),
    robots: "index, follow",
    ...over,
  };
}

export async function notFoundView(ctx: Ctx): Promise<View> {
  return {
    type: "view",
    chrome: await chrome(ctx, { title: "Page not found", robots: "noindex, follow" }),
    body: { type: "notfound" },
  };
}

/**
 * One page of a hub, newest first. Without a region, 21 are read, so the "Older" link exists only when there is
 * something older. With one, matching entries are kept until a page is full or SCAN_LIMIT entries have been examined,
 * and the cursor is the last entry examined, so the next page continues the scan.
 */
export async function readHub(
  reader: Reader,
  hub: string,
  opts: { cursor?: string; region?: string },
): Promise<{ entries: HubEntry[]; next: string | null }> {
  if (!opts.region) {
    const { items } = await reader.listHub(hub, { limit: HUB_PAGE + 1, cursor: opts.cursor });
    const entries = items.slice(0, HUB_PAGE);
    return { entries, next: items.length > HUB_PAGE ? encodeCursor(keyOf(entries[HUB_PAGE - 1] as never)) : null };
  }
  const entries: HubEntry[] = [];
  let examined = 0;
  let cursor: string | null | undefined = opts.cursor;
  let next: string | null = null;
  while (examined < SCAN_LIMIT) {
    const page = await reader.listHub(hub, { limit: Math.min(50, SCAN_LIMIT - examined), cursor });
    for (const entry of page.items) {
      examined++;
      if (entry.region === opts.region) entries.push(entry);
      if (entries.length === HUB_PAGE) return { entries, next: encodeCursor(keyOf(entry as never)) };
    }
    next = cursor = page.cursor;
    if (!cursor) break;
  }
  return { entries, next };
}

const olderHref = (path: string, next: string | null, region?: string) => {
  if (!next) return undefined;
  const q = new URLSearchParams();
  if (region) q.set("region", region);
  q.set("after", next);
  return `${path}?${q}`;
};

/** Before the first curation: no slots, so the lead is back-filled from the newest pages and the rest is Latest. */
const NO_CURATION: Curation = {
  schema: "curation.v1",
  lang: "en",
  created_at: "",
  approved_at: "",
  curator_model: "",
  slots: { lead: { page_id: "" }, side: [], sections: [] },
  checks: { passed: true, results: [] },
};

async function loadHome(ctx: Ctx, m: PageMatch): Promise<View> {
  const { reader } = ctx.data;
  const region = m.kind === "region" ? (m.segments[1] as string) : undefined;
  const [stored, tax] = await Promise.all([ctx.data.curation(), ctx.taxonomy()]);
  const curation = stored ?? NO_CURATION;
  const pages = await reader.getPages(slotsOf(curation).flatMap((s) => s.slot.page_id || []));
  const wanted = region ? [`region/${region}`] : [...new Set([...backfillHubs(curation, pages).values(), "latest"])];
  const hubs = new Map(
    await Promise.all(
      wanted.map(async (hub) => {
        const limit = hub === "latest" || region ? LATEST_READ : BACKFILL_DEPTH;
        return [hub, (await reader.listHub(hub, { limit })).items] as const;
      }),
    ),
  );
  const input = { curation, pages, hubs };
  const regionHub = hubs.get(`region/${region}`) ?? [];
  const model = region ? composeRegion(input, region, regionHub) : composeHome(input);
  const more =
    model.older && (regionHub.indexOf(model.older) < regionHub.length - 1 || regionHub.length >= LATEST_READ);
  return {
    type: "view",
    chrome: await chrome(ctx, {
      title: region ? regionLabel(region) : "Home",
      canonical: region ? `/region/${region}` : "/",
    }),
    body: {
      type: region ? "region" : "home",
      date: ctx.now.toISOString().slice(0, 10),
      now: ctx.now.toISOString(),
      selected: region ?? "all",
      regions: regionOptions(ctx.regions),
      model,
      names: Object.fromEntries(tax.map((n) => [n.id, n.name])),
      pages: await listModel(ctx.data, model),
      ...(model.older && more
        ? { olderHref: olderHref(`/region/${region}`, encodeCursor(keyOf(model.older as never))) }
        : {}),
      ...(region ? { region } : {}),
    },
  };
}

/** A hub that does not exist: a redirect (looked up only now, and only under a known first segment) or a 404. */
async function missing(ctx: Ctx, m: PageMatch, tax: TaxonomyNode[]): Promise<View> {
  const known = m.kind === "entity" || tax.some((n) => n.parent === null && n.id === m.segments[0]);
  const redirect = known ? await ctx.data.reader.getRedirect(canonicalPath(m.segments)) : undefined;
  return redirect ? { type: "redirect", location: redirect.to, status: redirect.status } : notFoundView(ctx);
}

/** `/latest`, a category hub, an entity hub, and a region view's older pages. */
async function loadHub(ctx: Ctx, m: PageMatch): Promise<View> {
  const { reader } = ctx.data;
  const tax = await ctx.taxonomy();
  const path = canonicalPath(m.segments);
  const slug = m.segments.join("/");
  const regionView = m.kind === "region" ? (m.segments[1] as string) : undefined;
  const kind = m.kind === "category" || m.kind === "entity" ? m.kind : "latest";
  let hub = "latest";
  let title = "Latest";
  let fly: string | undefined;
  let subcategories: NavLink[] = [];
  if (regionView) {
    hub = `region/${regionView}`;
    title = regionLabel(regionView);
  } else if (m.kind === "category") {
    const node = tax.find((n) => n.id === slug);
    if (!node) return missing(ctx, m, tax);
    hub = `cat/${slug}`;
    title = node.name;
    fly = tax.find((n) => n.id === node.parent)?.name;
    subcategories = tax.filter((n) => n.parent === slug).map((n) => ({ label: n.name, href: `/${n.id}` }));
  } else if (m.kind === "entity") {
    const id = m.segments[1] as string;
    const entity = await reader.getEntity(id);
    if (!entity) return missing(ctx, m, tax);
    hub = `ent/${id}`;
    title = entity.name;
    fly = entity.kind;
  }
  const { entries, next } = await readHub(reader, hub, { cursor: m.after, region: m.region });
  let strip: { page: ListedPage; figureIds: string[] } | undefined;
  const newest = entries[0];
  if (m.kind === "entity" && !m.after && newest) {
    const [d, s] = await Promise.all([
      ctx.data.pageJson(newest.path, "data.json", MAX_DATA_BYTES),
      ctx.data.pageJson(newest.path, "sources.json", MAX_DATA_BYTES),
    ]);
    const pd = PageData.safeParse(d);
    const ps = PageSources.safeParse(s);
    if (pd.success && ps.success)
      strip = {
        page: listedOf(newest, { data: new PageDataResolver(pd.data, ps.data) }),
        figureIds: Object.keys(pd.data.points).slice(0, STRIP_FIGURES),
      };
  }
  const first = !m.after && !m.region;
  const where = regionView ? `/region/${regionView}` : path;
  return {
    type: "view",
    chrome: await chrome(ctx, {
      title,
      ...(m.kind === "category" ? { current: `/${m.segments[0]}` } : {}),
      robots: first ? "index, follow" : "noindex, follow",
      ...(first ? { canonical: where } : {}),
    }),
    body: {
      type: "hub",
      kind,
      title,
      ...(fly ? { fly } : {}),
      subcategories,
      ...(m.kind === "category" || m.kind === "latest"
        ? { filter: { base: path, selected: m.region ?? "all", regions: regionOptions(ctx.regions) } }
        : {}),
      entries,
      ...(next ? { olderHref: olderHref(where, next, m.region) } : {}),
      ...(m.kind === "entity"
        ? {
            years: [...Map.groupBy(entries, (e) => e.published_at.slice(0, 4))].map(([year, list]) => ({
              year,
              entries: list,
            })),
          }
        : {}),
      ...(strip ? { strip } : {}),
    },
  };
}

export async function loadView(ctx: Ctx, m: PageMatch): Promise<View> {
  if (m.kind === "home" || (m.kind === "region" && !m.after)) return loadHome(ctx, m);
  return loadHub(ctx, m);
}
