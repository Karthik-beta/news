// Unit tests (inside workerd): PUT /v1/home/curation (table-driven rejections), rollback, taxonomy and redirects.
import type { Curation, PageRecord } from "@news/worker-shared/records";
import { curationPages, makeCuration, makePageRecord } from "@news/worker-shared/testing/fixtures";
import { homeUrls } from "@news/worker-shared/urls";
import { schemas } from "@news/worker-shared/validate";
import { describe, expect, it } from "vitest";
import { expectItemsValid, harness, ORIGIN } from "./testing";

function seeded(change?: (pages: PageRecord[]) => void) {
  const h = harness();
  const pages = curationPages();
  change?.(pages);
  h.table.seed(...(pages as never[]));
  return h;
}

const ids = curationPages().map((p) => p.page_id);
const withSlots = (mutate: (c: Curation) => void) => {
  const c = makeCuration();
  mutate(c);
  return c;
};

interface Case {
  name: string;
  code: string;
  detail?: string;
  pages?: (pages: PageRecord[]) => void;
  curation?: Curation;
  seed?: (h: ReturnType<typeof harness>) => void;
}

const CASES: Case[] = [
  {
    name: "a page that does not exist",
    code: "invalid_curation",
    detail: "page_missing",
    curation: withSlots((c) => {
      c.slots.side[0] = { page_id: "no-such-page" };
    }),
  },
  {
    name: "an unpublished page (versions but no live record)",
    code: "invalid_curation",
    detail: "page_unpublished",
    curation: withSlots((c) => {
      c.slots.side[0] = { page_id: "half-published" };
    }),
    seed: (h) => h.table.seed({ ...makePageRecord({ id: "half-published" }), SK: "V#000001" } as never),
  },
  {
    name: "a withdrawn page",
    code: "invalid_curation",
    detail: "page_withdrawn",
    pages: (p) => {
      (p[4] as PageRecord).status = "withdrawn";
    },
  },
  {
    name: "a page used twice",
    code: "invalid_curation",
    detail: "duplicate_page",
    curation: withSlots((c) => {
      c.slots.side[1] = { page_id: c.slots.lead.page_id };
    }),
  },
  {
    name: "one side story instead of two",
    code: "invalid_curation",
    detail: "schema.minItems",
    curation: withSlots((c) => {
      c.slots.side.pop();
    }),
  },
  {
    name: "three side stories",
    code: "invalid_curation",
    detail: "schema.maxItems",
    curation: withSlots((c) => {
      c.slots.side.push({ page_id: "x-extra" });
    }),
  },
  {
    name: "two sections",
    code: "invalid_curation",
    detail: "schema.minItems",
    curation: withSlots((c) => {
      c.slots.sections.pop();
    }),
  },
  {
    name: "six sections",
    code: "invalid_curation",
    detail: "schema.maxItems",
    curation: withSlots((c) => {
      const s = c.slots.sections[0] as Curation["slots"]["sections"][0];
      c.slots.sections.push(s, s, s);
    }),
  },
  {
    name: "a section with two stories",
    code: "invalid_curation",
    detail: "schema.minItems",
    curation: withSlots((c) => {
      c.slots.sections[0]?.pages.pop();
    }),
  },
  {
    name: "a section with five stories",
    code: "invalid_curation",
    detail: "schema.maxItems",
    curation: withSlots((c) => {
      c.slots.sections[0]?.pages.push({ page_id: "a1" }, { page_id: "a2" });
    }),
  },
  {
    name: "fewer than three categories",
    code: "invalid_curation",
    detail: "breadth_categories",
    pages: (p) => {
      for (const [i, x] of p.entries()) x.categories = [i % 2 ? "economy" : "science"];
    },
  },
  {
    name: "fewer than three regions",
    code: "invalid_curation",
    detail: "breadth_regions",
    pages: (p) => {
      for (const [i, x] of p.entries()) x.region = i % 2 ? "europe" : "india";
    },
  },
  {
    name: "three stories about one entity",
    code: "invalid_curation",
    detail: "breadth_entity",
    pages: (p) => {
      for (const x of p.slice(0, 3)) x.entities = [{ id: "company:apple", name: "Apple", kind: "company" }];
    },
  },
  {
    name: "only finance sections",
    code: "invalid_curation",
    detail: "breadth_non_finance",
    curation: withSlots((c) => {
      for (const [i, s] of c.slots.sections.entries()) s.category = i ? "finance/markets" : "finance";
    }),
  },
  {
    name: "a stale lead",
    code: "invalid_curation",
    detail: "lead_stale",
    pages: (p) => ((p[0] as PageRecord).published_at = "2026-10-20T00:00:00Z"),
  },
  {
    name: "a figure that is not the page's",
    code: "invalid_curation",
    detail: "figure_not_on_page",
    curation: withSlots((c) => {
      c.slots.lead.figure_id = "dp:invented";
    }),
  },
  {
    name: "a rationale (it stays on the laptop)",
    code: "invalid_curation",
    detail: "schema.additionalProperties",
    curation: { ...makeCuration(), rationale: "because" } as Curation,
  },
];

describe("PUT /v1/home/curation rejects", () => {
  it.each(CASES)("$name → 400 $detail, nothing written", async (c) => {
    const h = seeded(c.pages);
    c.seed?.(h);
    const res = await h.call("PUT", "/v1/home/curation", { body: c.curation ?? makeCuration(ids) });
    expect(res.status).toBe(400);
    const body = (await res.json()) as { error: { code: string; details?: { code: string }[] } };
    expect(schemas.validateErrorBody(body)).toBe(true);
    expect(body.error.code).toBe(c.code);
    expect(body.error.details?.map((d) => d.code)).toContain(c.detail);
    expect(h.writeCount()).toBe(0);
  });
});

describe("PUT /v1/home/curation", () => {
  it("writes V#1, moves LATEST and purges exactly home and the region views", async () => {
    const h = seeded();
    const res = await h.call("PUT", "/v1/home/curation", { body: makeCuration() });
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(schemas.validateCurationResponse(body)).toBe(true);
    expect(body).toMatchObject({ version: 1, live_version: 1 });
    const items = h.table.dump().filter((i) => i.PK === "HOME#CURATION");
    expectItemsValid(items);
    expect(items.map((i) => i.SK).sort()).toEqual(["LATEST", "V#000001"]);
    expect(items.find((i) => i.SK === "LATEST")).toMatchObject({
      version: 1,
      lang: "en",
      made_live_at: "2026-11-12T10:00:00.000Z",
    });
    expect(h.purger.calls).toEqual([homeUrls(ORIGIN, h.deps.regions)]);
    expect(h.search.calls).toEqual([]);
    expect(h.bucket.writes).toEqual([]);
  });

  it("each publish is a new version", async () => {
    const h = seeded();
    for (const v of [1, 2, 3]) {
      const res = await h.call("PUT", "/v1/home/curation", { body: makeCuration() });
      expect(await res.json()).toMatchObject({ version: v, live_version: v });
    }
  });
});

describe("POST /v1/home/curation/rollback", () => {
  it("moves LATEST to the earlier version and purges exactly the home and region-view URLs", async () => {
    const h = seeded();
    await h.call("PUT", "/v1/home/curation", { body: makeCuration() });
    const v2 = makeCuration();
    v2.slots.lead.figure_id = "dp:cpi-mom";
    await h.call("PUT", "/v1/home/curation", { body: v2 });
    h.purger.calls.length = 0;

    const res = await h.call("POST", "/v1/home/curation/rollback", { body: { version: 1 } });
    expect(res.status).toBe(200);
    expect(await res.json()).toEqual({ version: 2, live_version: 1, purged: homeUrls(ORIGIN, h.deps.regions) });
    const latest = h.table.dump().find((i) => i.PK === "HOME#CURATION" && i.SK === "LATEST");
    expect(latest).toMatchObject({ version: 1 });
    const slots = latest?.slots as unknown as Curation["slots"] | undefined;
    expect(slots?.lead.figure_id).toBe("dp:cpi-yoy");
    expect(h.purger.calls).toEqual([[`${ORIGIN}/`, ...h.deps.regions.map((r) => `${ORIGIN}/region/${r}`)]]);
    expectItemsValid(h.table.dump().filter((i) => i.PK === "HOME#CURATION"));

    // The next publish continues after the newest version, not after the live one.
    const next = await h.call("PUT", "/v1/home/curation", { body: makeCuration() });
    expect(await next.json()).toMatchObject({ version: 3, live_version: 3 });
  });

  it("404 for a version that does not exist; 400 for a bad body", async () => {
    const h = seeded();
    expect((await h.call("POST", "/v1/home/curation/rollback", { body: { version: 9 } })).status).toBe(404);
    expect((await h.call("POST", "/v1/home/curation/rollback", { body: { version: "1" } })).status).toBe(400);
    expect(h.writeCount()).toBe(0);
  });
});

describe("PUT /v1/taxonomy and /v1/redirects", () => {
  it("writes taxonomy nodes as one tree and refuses inconsistent parents", async () => {
    const h = harness();
    const ok = await h.call("PUT", "/v1/taxonomy", {
      body: {
        nodes: [
          { id: "economy", name: "Economy", parent: null, order: 1, lang: "en" },
          { id: "economy/inflation", name: "Inflation", parent: "economy", order: 1, lang: "en" },
        ],
      },
    });
    expect(ok.status).toBe(200);
    expect(await ok.json()).toEqual({ written: 2, purged: [`${ORIGIN}/economy`, `${ORIGIN}/economy/inflation`] });
    expectItemsValid(h.table.dump().filter((i) => i.PK === "TAX#tree"));
    const bad = await h.call("PUT", "/v1/taxonomy", {
      body: { nodes: [{ id: "science/space", name: "Space", parent: "economy", order: 1, lang: "en" }] },
    });
    expect(bad.status).toBe(400);
    const orphan = await h.call("PUT", "/v1/taxonomy", {
      body: { nodes: [{ id: "science/space", name: "Space", parent: "science", order: 1, lang: "en" }] },
    });
    expect(orphan.status).toBe(400);
  });

  it.each(["latest", "region", "mcp", "feeds", "corrections", "2025-outlook", "7days"])(
    "refuses the top-level slug %s (a site route, or not starting with a letter)",
    async (id) => {
      const h = harness();
      const res = await h.call("PUT", "/v1/taxonomy", {
        body: { nodes: [{ id, name: "X", parent: null, order: 1, lang: "en" }] },
      });
      expect(res.status).toBe(400);
      const body = (await res.json()) as { error: { details: { code: string }[] } };
      expect(body.error.details.map((d) => d.code)).toContain("taxonomy.reserved_slug");
      expect(h.writeCount()).toBe(0);
    },
  );

  it("allows a reserved word below a category and a digit after the first letter", async () => {
    const h = harness();
    const res = await h.call("PUT", "/v1/taxonomy", {
      body: {
        nodes: [
          { id: "economy", name: "Economy", parent: null, order: 1, lang: "en" },
          { id: "economy/latest", name: "Latest", parent: "economy", order: 1, lang: "en" },
          { id: "g7", name: "G7", parent: null, order: 2, lang: "en" },
        ],
      },
    });
    expect(res.status).toBe(200);
  });

  it("a large invalid taxonomy lists at most 50 problems, within the error contract", async () => {
    const h = harness();
    const nodes = Array.from({ length: 100 }, (_, i) => ({
      id: `science/topic-${i}`,
      name: `Topic ${i}`,
      parent: "economy",
      order: i,
      lang: "en",
    }));
    const res = await h.call("PUT", "/v1/taxonomy", { body: { nodes } });
    expect(res.status).toBe(400);
    const body = (await res.json()) as { error: { details: unknown[] } };
    expect(body.error.details).toHaveLength(50);
    expect(schemas.validateErrorBody(body)).toBe(true);
  });

  it("refuses a redirect whose target is itself redirected, including by an earlier request", async () => {
    const h = harness();
    const put = (from: string, to: string) =>
      h.call("PUT", "/v1/redirects", { body: { redirects: [{ from, to, status: 301 }] } });
    expect((await put("/a", "/b")).status).toBe(200);
    // /b → /a would close the loop /a → /b → /a; /c → /a would make a chain.
    for (const [from, to] of [
      ["/b", "/a"],
      ["/c", "/a"],
    ]) {
      const res = await put(from, to);
      expect(res.status, `${from} → ${to}`).toBe(400);
      expect(((await res.json()) as { error: { details: { code: string }[] } }).error.details[0]?.code).toBe(
        "redirect.chain",
      );
    }
    expect(h.table.dump().filter((i) => i.type === "redirect")).toHaveLength(1);
  });

  it("two requests racing to close a loop cannot both win", async () => {
    const h = harness();
    // Both requests have passed their checks (nothing redirected yet) before either writes.
    const transact = h.table.transact.bind(h.table);
    let second: Promise<Response> | undefined;
    h.table.transact = async (ops) => {
      if (!second)
        second = h.call("PUT", "/v1/redirects", { body: { redirects: [{ from: "/b", to: "/a", status: 301 }] } });
      return transact(ops);
    };
    const first = await h.call("PUT", "/v1/redirects", {
      body: { redirects: [{ from: "/a", to: "/b", status: 301 }] },
    });
    expect(second).toBeDefined();
    const statuses = [first.status, (await second)?.status].sort();
    expect(statuses[0]).toBe(200);
    expect(statuses[1]).not.toBe(200);
    expect(h.table.dump().filter((i) => i.type === "redirect")).toHaveLength(1);
  });

  it("writes redirects and refuses loops and unsafe paths", async () => {
    const h = harness();
    const ok = await h.call("PUT", "/v1/redirects", {
      body: { redirects: [{ from: "/economics", to: "/economy", status: 301 }] },
    });
    expect(ok.status).toBe(200);
    expectItemsValid(h.table.dump().filter((i) => i.type === "redirect"));
    for (const redirects of [
      [{ from: "/a", to: "/a", status: 301 }],
      [{ from: "/a/../b", to: "/c", status: 301 }],
      [{ from: "https://evil.example/", to: "/c", status: 301 }],
      [{ from: "/a", to: "//evil.example", status: 301 }],
      [{ from: "/a", to: "/b", status: 302 }],
    ]) {
      expect((await h.call("PUT", "/v1/redirects", { body: { redirects } })).status).toBe(400);
    }
  });
});
