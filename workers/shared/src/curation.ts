// The home curation's code checks (TD "Home page curation", step 3; ADR 0005). `news home check` runs them on the
// laptop; the write API runs them again on PUT /v1/home/curation against the live page records, so a curation that
// was fine when drafted but points at a page withdrawn since is refused.
import type { Curation, CurationSlot, PageRecord } from "./records";
import type { Issue } from "./validate";

export const RULES = {
  minCategories: 3,
  minRegions: 3,
  maxStoriesPerEntity: 2,
  leadMaxAgeDays: 7,
  sides: 2,
  minSections: 3,
  maxSections: 5,
  minPerSection: 3,
  maxPerSection: 4,
} as const;

/** What the checks know about a page: its live record, or why there is none. */
export type PageLookup = { state: "live"; page: PageRecord } | { state: "missing" } | { state: "unpublished" };

interface Placed {
  slot: CurationSlot;
  path: string;
}

/** Every slot with its JSON Pointer, in reading order: lead, side stories, then sections. */
export function slotsOf(c: Curation): Placed[] {
  return [
    { slot: c.slots.lead, path: "/slots/lead" },
    ...c.slots.side.map((slot, i) => ({ slot, path: `/slots/side/${i}` })),
    ...c.slots.sections.flatMap((s, i) =>
      s.pages.map((slot, j) => ({ slot, path: `/slots/sections/${i}/pages/${j}` })),
    ),
  ];
}

export const topLevel = (slug: string) => slug.split("/")[0] as string;
const DAY_MS = 86_400_000;

export function checkCuration(c: Curation, pages: Map<string, PageLookup>, now: Date): Issue[] {
  const issues: Issue[] = [];
  const fail = (code: string, message: string, path?: string) =>
    issues.push({ code, message, ...(path ? { path } : {}) });

  // Slot counts (the schema already enforces them; checked again so this function stands on its own).
  if (c.slots.side.length !== RULES.sides)
    fail("slot_count", `need exactly ${RULES.sides} side stories`, "/slots/side");
  if (c.slots.sections.length < RULES.minSections || c.slots.sections.length > RULES.maxSections)
    fail("slot_count", `need ${RULES.minSections}–${RULES.maxSections} sections`, "/slots/sections");
  c.slots.sections.forEach((s, i) => {
    if (s.pages.length < RULES.minPerSection || s.pages.length > RULES.maxPerSection)
      fail(
        "slot_count",
        `a section needs ${RULES.minPerSection}–${RULES.maxPerSection} stories`,
        `/slots/sections/${i}/pages`,
      );
  });

  const placed = slotsOf(c);
  const seen = new Map<string, string>();
  const live: PageRecord[] = [];
  for (const { slot, path } of placed) {
    const first = seen.get(slot.page_id);
    if (first) fail("duplicate_page", `page ${slot.page_id} is already used at ${first}`, path);
    else seen.set(slot.page_id, path);

    const lookup = pages.get(slot.page_id) ?? { state: "missing" };
    if (lookup.state === "missing") {
      fail("page_missing", `page ${slot.page_id} does not exist`, path);
      continue;
    }
    if (lookup.state === "unpublished") {
      fail("page_unpublished", `page ${slot.page_id} has no live version`, path);
      continue;
    }
    const page = lookup.page;
    if (page.status === "withdrawn") {
      fail("page_withdrawn", `page ${slot.page_id} is withdrawn`, path);
      continue;
    }
    if (page.status !== "published") {
      fail("page_unpublished", `page ${slot.page_id} is not published`, path);
      continue;
    }
    if (!first) live.push(page);
    if (slot.figure_id !== undefined && !page.data_ids.includes(slot.figure_id))
      fail("figure_not_on_page", `figure ${slot.figure_id} is not in page ${slot.page_id}'s data`, `${path}/figure_id`);
    if (slot.chart_id !== undefined && !page.chart_ids.includes(slot.chart_id))
      fail(
        "chart_not_on_page",
        `chart ${slot.chart_id} is not one of page ${slot.page_id}'s charts`,
        `${path}/chart_id`,
      );
  }

  // Breadth, over the pages that resolved.
  const categories = new Set(live.map((p) => topLevel(p.categories[0] ?? "")));
  if (categories.size < RULES.minCategories)
    fail(
      "breadth_categories",
      `curated pages span ${categories.size} categories; at least ${RULES.minCategories} needed`,
    );
  const regions = new Set(live.map((p) => p.region));
  if (regions.size < RULES.minRegions)
    fail("breadth_regions", `curated pages span ${regions.size} regions; at least ${RULES.minRegions} needed`);
  const perEntity = new Map<string, number>();
  for (const p of live)
    for (const e of new Set(p.entities.map((x) => x.id))) perEntity.set(e, (perEntity.get(e) ?? 0) + 1);
  for (const [entity, count] of perEntity) {
    if (count > RULES.maxStoriesPerEntity)
      fail("breadth_entity", `${count} stories are about ${entity}; at most ${RULES.maxStoriesPerEntity} allowed`);
  }
  if (!c.slots.sections.some((s) => topLevel(s.category) !== "finance"))
    fail("breadth_non_finance", "at least one section must be outside finance", "/slots/sections");

  // Freshness of the lead.
  const lead = pages.get(c.slots.lead.page_id);
  if (lead?.state === "live" && lead.page.status === "published") {
    const ageDays = (now.getTime() - Date.parse(lead.page.published_at)) / DAY_MS;
    const excused = c.slots.lead.still_leading === true && lead.page.tier === 1;
    if (ageDays > RULES.leadMaxAgeDays && !excused)
      fail(
        "lead_stale",
        `the lead is ${Math.floor(ageDays)} days old; only a Tier 1 page marked still_leading may be older than ${RULES.leadMaxAgeDays}`,
        "/slots/lead",
      );
    if (c.slots.lead.still_leading === true && lead.page.tier !== 1)
      fail("lead_stale", "only a Tier 1 page can be marked still_leading", "/slots/lead/still_leading");
  }
  return issues;
}
