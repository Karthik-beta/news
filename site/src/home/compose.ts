// The home page's render-time rules (ADR 0005, "Between curations"; ADR 0010, section 5). Pure functions over data the
// loader has already read, so the fixture sets test them without storage. A curation names pages, never text: every
// word of a slot is the page record's (or its hub entry's, for a back-filled slot).
import { slotsOf } from "@news/worker-shared/curation";
import type { Curation, HubEntry, PageRecord } from "@news/worker-shared/records";

/** The fields of a page that a story shows: a page record has them, and so does its hub entry. */
export type Story = Pick<
  PageRecord,
  "page_id" | "path" | "title" | "fly_title" | "summary" | "page_type" | "region" | "published_at" | "tier"
>;

/** A slot's place in the curation, as a JSON Pointer: "/slots/lead", "/slots/side/1", "/slots/sections/0/pages/2". */
export type SlotRef = string;

export interface Slot {
  story: Story;
  /** The page's own key figure or chart the curation chose. Never set on a back-filled slot. */
  figure?: string;
  chart?: string;
  backfilled: boolean;
}

export interface HomeInput {
  curation: Curation;
  /** The live records of the curation's pages (a withdrawn page's record is there; a missing page is not). */
  pages: Map<string, PageRecord>;
  /** Newest first: each back-fill hub, read 20 deep, and "latest", read 30 deep. Keyed by hub ID. */
  hubs: Map<string, HubEntry[]>;
}

export interface HomeModel {
  lead: Slot | undefined;
  side: Slot[];
  /** A section's category slug and its stories; empty sections are left out. */
  sections: { category: string; slots: Slot[] }[];
  /** The newest pages that are in no slot, newest first. */
  latest: HubEntry[];
  /** Region views: the last hub entry examined, which the "Older" link continues from. */
  older?: HubEntry;
}

export const LATEST_SHOWN = 10;
export const BACKFILL_DEPTH = 20;
export const REGION_STORIES = 12;
const LATEST_HUB = "latest";

const isLive = (p: PageRecord | undefined): p is PageRecord => p?.status === "published";

/** The hub to read for each slot whose page is missing or withdrawn: its section's category, or the dead page's first one. */
export function backfillHubs(c: Curation, pages: Map<string, PageRecord>): Map<SlotRef, string> {
  const out = new Map<SlotRef, string>();
  const sections = new Map<string, string>(
    c.slots.sections.flatMap((s, i) => s.pages.map((_, j) => [`/slots/sections/${i}/pages/${j}`, s.category] as const)),
  );
  for (const { slot, path } of slotsOf(c)) {
    const page = pages.get(slot.page_id);
    if (page?.status === "published") continue;
    const category = sections.get(path) ?? page?.categories[0];
    out.set(path, category ? `cat/${category}` : LATEST_HUB);
  }
  return out;
}

/** Lowest tier first, then newest, then page ID, so the choice never depends on the read order. */
const better = (a: HubEntry, b: HubEntry) =>
  a.tier - b.tier ||
  (a.published_at < b.published_at ? 1 : a.published_at > b.published_at ? -1 : 0) ||
  a.page_id.localeCompare(b.page_id);

function best(entries: HubEntry[] | undefined, shown: Set<string>): HubEntry | undefined {
  return (entries ?? [])
    .slice(0, BACKFILL_DEPTH)
    .filter((e) => !shown.has(e.page_id))
    .sort(better)[0];
}

export function composeHome(input: HomeInput): HomeModel {
  const { curation: c, pages, hubs } = input;
  const hubFor = backfillHubs(c, pages);
  const placed = slotsOf(c);
  const shown = new Set<string>();
  const slots = new Map<SlotRef, Slot>();
  // Live pages first, so a back-fill never takes a page that a later slot shows.
  for (const { slot, path } of placed) {
    const page = pages.get(slot.page_id);
    if (!isLive(page) || shown.has(page.page_id)) continue;
    shown.add(page.page_id);
    slots.set(path, {
      story: page,
      ...(slot.figure_id ? { figure: slot.figure_id } : {}),
      ...(slot.chart_id ? { chart: slot.chart_id } : {}),
      backfilled: false,
    });
  }
  for (const { path } of placed) {
    const hub = hubFor.get(path);
    if (hub === undefined) continue;
    // The lead falls back to the newest pages overall; any other slot is dropped when its hub has nothing left.
    const pick = best(hubs.get(hub), shown) ?? (path === "/slots/lead" ? best(hubs.get(LATEST_HUB), shown) : undefined);
    if (!pick) continue;
    shown.add(pick.page_id);
    slots.set(path, { story: pick, backfilled: true });
  }
  const get = (path: string) => slots.get(path);
  return {
    lead: get("/slots/lead"),
    side: c.slots.side.flatMap((_, i) => get(`/slots/side/${i}`) ?? []),
    sections: c.slots.sections
      .map((s, i) => ({
        category: s.category,
        slots: s.pages.flatMap((_, j) => get(`/slots/sections/${i}/pages/${j}`) ?? []),
      }))
      .filter((s) => s.slots.length > 0),
    latest: (hubs.get(LATEST_HUB) ?? []).filter((e) => !shown.has(e.page_id)).slice(0, LATEST_SHOWN),
  };
}

/**
 * A region view: the curated stories whose page is in `region`, in curation order, then the newest entries of the
 * region's hub that are not shown yet, up to 12. Withdrawn pages are not back-filled here: the hub top-up does that.
 */
export function composeRegion(input: HomeInput, region: string, regionEntries: HubEntry[]): HomeModel {
  const shown = new Set<string>();
  const stories: Slot[] = [];
  for (const { slot } of slotsOf(input.curation)) {
    const page = input.pages.get(slot.page_id);
    if (!isLive(page) || page.region !== region || shown.has(page.page_id) || stories.length >= REGION_STORIES)
      continue;
    shown.add(page.page_id);
    stories.push({
      story: page,
      ...(slot.figure_id ? { figure: slot.figure_id } : {}),
      ...(slot.chart_id ? { chart: slot.chart_id } : {}),
      backfilled: false,
    });
  }
  let older: HubEntry | undefined;
  for (const entry of regionEntries) {
    if (stories.length >= REGION_STORIES) break;
    older = entry;
    if (shown.has(entry.page_id)) continue;
    shown.add(entry.page_id);
    stories.push({ story: entry, backfilled: true });
  }
  return {
    lead: stories[0],
    side: stories.slice(1, 3),
    sections: stories.length > 3 ? [{ category: "", slots: stories.slice(3) }] : [],
    latest: [],
    ...(older ? { older } : {}),
  };
}
