// ADR 0010 section 5 over the four fixture sets: back-fill, the lead's fallback, the region top-up, Latest.
import { slotsOf } from "@news/worker-shared/curation";
import type { Doc } from "@news/worker-shared/dynamo/table";
import type { CurationRecord, HubEntry, PageRecord } from "@news/worker-shared/records";
import { describe, expect, it } from "vitest";
import { FIXTURE_ITEMS } from "../../tests/support/site-data";
import { backfillHubs, composeHome, composeRegion, type HomeInput } from "./compose";

/** What the loader reads for a set: the curation's page records and each hub, newest first. */
function inputOf(set: string, edit: (items: Doc[]) => Doc[] = (i) => i): HomeInput {
  const items = edit(structuredClone(FIXTURE_ITEMS[set] as Doc[]));
  const curation = items.find((i) => i.PK === "HOME#CURATION") as unknown as CurationRecord;
  const records = new Map(
    items.filter((i) => i.type === "page").map((i) => [i.page_id as string, i as unknown as PageRecord]),
  );
  const hubs = new Map<string, HubEntry[]>();
  for (const i of items.filter((x) => x.type === "hub_entry") as unknown as HubEntry[]) {
    hubs.set(i.hub, [...(hubs.get(i.hub) ?? []), i]);
  }
  for (const [hub, list] of hubs)
    hubs.set(
      hub,
      list.sort((a, b) => (a.SK < b.SK ? 1 : -1)),
    );
  const ids = slotsOf(curation).map((s) => s.slot.page_id);
  return {
    curation,
    pages: new Map(ids.flatMap((id) => (records.has(id) ? [[id, records.get(id) as PageRecord]] : []))),
    hubs,
  };
}

const shownIds = (m: ReturnType<typeof composeHome>) =>
  [m.lead, ...m.side, ...m.sections.flatMap((s) => s.slots)].flatMap((s) => s?.story.page_id ?? []);

describe("composeHome: the normal set", () => {
  const input = inputOf("normal");
  const model = composeHome(input);

  it("shows the curation as it is: a lead, two side stories, four sections of three", () => {
    expect(model.lead?.story.page_id).toBe("apple-fy2024-results");
    expect(model.lead?.figure).toBe("dp_aapl_net_sales_fy2024");
    expect(model.side.map((s) => s.story.page_id)).toEqual(["fed-december-2024", "chipmaker-capacity"]);
    expect(model.side[0]?.chart).toBe("path");
    expect(model.sections.map((s) => [s.category, s.slots.length])).toEqual([
      ["economy", 3],
      ["business", 3],
      ["science", 3],
      ["tech", 3],
    ]);
    expect(shownIds(model).every((id, i, all) => all.indexOf(id) === i)).toBe(true);
    expect(shownIds(model).length).toBe(15);
    expect(model.lead?.backfilled).toBe(false);
  });

  it("Latest is the newest ten pages that are in no slot", () => {
    expect(model.latest).toHaveLength(10);
    const slotIds = new Set(shownIds(model));
    expect(model.latest.some((e) => slotIds.has(e.page_id))).toBe(false);
    const times = model.latest.map((e) => e.published_at);
    expect(times).toEqual([...times].sort().reverse());
    // The newest page of the set that is in no slot is the first.
    const newest = [...(input.hubs.get("latest") ?? [])].find((e) => !slotIds.has(e.page_id));
    expect(model.latest[0]?.page_id).toBe(newest?.page_id);
  });

  it("every word of a slot is the page record's, and an updated record changes the slot", () => {
    for (const slot of [model.lead, ...model.side, ...model.sections.flatMap((s) => s.slots)]) {
      const record = input.pages.get(slot?.story.page_id as string) as PageRecord;
      expect(slot?.story).toMatchObject({
        title: record.title,
        fly_title: record.fly_title,
        summary: record.summary,
        path: record.path,
        tier: record.tier,
      });
    }
    const edited = inputOf("normal", (items) =>
      items.map((i) =>
        i.page_id === "apple-fy2024-results" && i.type === "page" ? { ...i, title: "A new headline" } : i,
      ),
    );
    expect(composeHome(edited).lead?.story.title).toBe("A new headline");
  });
});

describe("composeHome: back-fill", () => {
  const input = inputOf("withdrawn-lead");

  it("a withdrawn lead reads its own first category's hub", () => {
    expect([...backfillHubs(input.curation, input.pages)]).toEqual([["/slots/lead", "cat/finance/earnings"]]);
  });

  it("picks the lowest tier, then the newest, that is not already on the page; no figure, no chart", () => {
    const model = composeHome(input);
    expect(model.lead?.backfilled).toBe(true);
    // Both finance/earnings candidates are tier 2; the bank is the newer.
    expect(model.lead?.story.page_id).toBe("bank-earnings-europe");
    expect(model.lead?.figure).toBeUndefined();
    expect(model.lead?.chart).toBeUndefined();
    expect(shownIds(model)).toHaveLength(15);
    expect(shownIds(model)).not.toContain("apple-fy2024-results");
    expect(model.latest.some((e) => e.page_id === "bank-earnings-europe")).toBe(false);
  });

  it("a section slot reads its section's hub, never a page already shown, and a slot with no candidate is dropped", () => {
    // Withdraw an economy section page: the economy hub's best candidate not on the page is chosen.
    const withdrawn = inputOf("normal", (items) =>
      items.map((i) =>
        i.page_id === "us-cpi-december-2024" && i.type === "page" ? { ...i, status: "withdrawn", hubs: [] } : i,
      ),
    );
    const model = composeHome(withdrawn);
    expect(backfillHubs(withdrawn.curation, withdrawn.pages).get("/slots/sections/0/pages/0")).toBe("cat/economy");
    const economy = model.sections[0];
    expect(economy?.slots).toHaveLength(3);
    expect(economy?.slots[0]?.backfilled).toBe(true);
    expect(shownIds(model)).toHaveLength(new Set(shownIds(model)).size);

    // Nothing left in the hub: the slot is dropped, and a section left empty is hidden.
    const bare = { ...withdrawn, hubs: new Map([["latest", withdrawn.hubs.get("latest") ?? []]]) };
    const dropped = composeHome(bare);
    expect(dropped.sections.map((s) => s.slots.length)).toEqual([2, 3, 3, 3]);
    const gone = ["us-cpi-december-2024", "china-industrial-output", "india-gdp-estimate"];
    const none = composeHome({ ...bare, pages: new Map([...bare.pages].filter(([id]) => !gone.includes(id))) });
    expect(none.sections.map((s) => s.category)).toEqual(["business", "science", "tech"]);
  });

  it("a lead whose category hub is empty falls back to the newest pages overall", () => {
    const empty = { ...input, hubs: new Map([["latest", input.hubs.get("latest") ?? []]]) };
    const model = composeHome(empty);
    expect(model.lead?.backfilled).toBe(true);
    expect(model.lead?.story.page_id).not.toBe("apple-fy2024-results");
  });

  it("a record that is gone (not just withdrawn) reads the latest hub", () => {
    const gone = inputOf("normal");
    gone.pages.delete("apple-fy2024-results");
    expect(backfillHubs(gone.curation, gone.pages).get("/slots/lead")).toBe("latest");
  });
});

describe("composeHome: an empty Latest", () => {
  it("has no list when every published page is in a slot", () => {
    const model = composeHome(inputOf("empty-latest"));
    expect(shownIds(model)).toHaveLength(15);
    expect(model.latest).toEqual([]);
  });

  it("shows fewer than ten when fewer exist", () => {
    const input = inputOf("normal");
    const latest = input.hubs.get("latest") ?? [];
    const slotIds = new Set(shownIds(composeHome(input)));
    input.hubs.set(
      "latest",
      latest.filter(
        (e) => slotIds.has(e.page_id) || e.page_id === "market-wrap-india" || e.page_id === "bond-yields-global",
      ),
    );
    expect(composeHome(input).latest.map((e) => e.page_id)).toEqual(["market-wrap-india", "bond-yields-global"]);
  });
});

describe("composeRegion", () => {
  const input = inputOf("region");
  const india = (input.hubs.get("region/india") ?? []).slice(0, 30);

  it("shows the curated stories of the region in curation order, then tops up from its hub to 12", () => {
    const view = composeRegion(input, "india", india);
    const curated = slotsOf(input.curation)
      .map((s) => input.pages.get(s.slot.page_id))
      .filter((p) => p?.region === "india")
      .map((p) => p?.page_id);
    const stories = [view.lead, ...view.side, ...view.sections.flatMap((s) => s.slots)];
    expect(stories).toHaveLength(12);
    expect(stories.slice(0, curated.length).map((s) => s?.story.page_id)).toEqual(curated);
    expect(stories.slice(0, curated.length).every((s) => s?.backfilled === false)).toBe(true);
    expect(stories.slice(curated.length).every((s) => s?.backfilled === true)).toBe(true);
    expect(new Set(stories.map((s) => s?.story.page_id)).size).toBe(12);
    expect(stories.every((s) => s?.story.region === "india")).toBe(true);
    expect(view.older).toBeDefined();
  });

  it("with fewer pages than 12 shows them all and no Older link target beyond the hub", () => {
    const view = composeRegion(
      inputOf("normal"),
      "australia",
      (inputOf("normal").hubs.get("region/australia") ?? []).slice(0, 30),
    );
    const count = [view.lead, ...view.side, ...view.sections.flatMap((s) => s.slots)].length;
    expect(count).toBeGreaterThan(0);
    expect(count).toBeLessThan(12);
  });
});
