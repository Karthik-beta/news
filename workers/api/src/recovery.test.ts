// Unit tests (inside workerd): retries after a partial failure finish the job, and concurrent operations on one page
// cannot leave its files, hubs or search row out of step with its live record.

import { sha256Hex } from "@news/worker-shared/crypto";
import { OutcomeUnknownError } from "@news/worker-shared/dynamo/table";
import { curationPages, makeBundle, makeCuration, makeMeta } from "@news/worker-shared/testing/fixtures";
import { describe, expect, it } from "vitest";
import { executeIndex } from "./pages";
import { expectItemsValid, harness, ORIGIN } from "./testing";

type H = ReturnType<typeof harness>;

const errorCode = async (r: Response) => ((await r.clone().json()) as { error?: { code: string } }).error?.code;
const items = (h: H, prefix: string) => h.table.dump().filter((i) => String(i.PK).startsWith(prefix));
const latest = (h: H, id = "test-page") => h.table.dump().find((i) => i.PK === `PAGE#${id}` && i.SK === "LATEST");

async function published(h: H) {
  const res = await h.call("PUT", "/v1/pages/test-page", { body: await makeBundle() });
  expect(res.status).toBe(200);
}

/** Makes the first batch write with a `kind` of an item whose key starts with `prefix` fail as throttled. */
function throttleOnce(h: H, kind: "put" | "delete", prefix: string) {
  const original = h.table.batchWrite.bind(h.table);
  let done = false;
  h.table.batchWrite = async (ops) => {
    const hit = ops.some((op) =>
      "put" in op
        ? kind === "put" && String(op.put.PK).startsWith(prefix)
        : kind === "delete" && op.delete.PK.startsWith(prefix),
    );
    if (!done && hit) {
      done = true;
      h.table.throttleNext = 1;
    }
    return original(ops);
  };
}

describe("withdrawal is resumable", () => {
  it("a failed purge → 502; the retry with the same key purges and returns 200", async () => {
    const h = harness();
    await published(h);
    h.purger.failNext = true;
    const first = await h.call("POST", "/v1/pages/test-page/withdraw", {
      body: { reason: "wrong figure" },
      key: "withdraw-0001",
    });
    expect([first.status, await errorCode(first)]).toEqual([502, "purge_failed"]);
    const retry = await h.call("POST", "/v1/pages/test-page/withdraw", {
      body: { reason: "wrong figure" },
      key: "withdraw-0001",
    });
    expect(retry.status).toBe(200);
    expect(((await retry.json()) as { purged: string[] }).purged).toEqual(
      expect.arrayContaining([`${ORIGIN}/2026/11/test-page`, `${ORIGIN}/2026/11/test-page/index.html`]),
    );
  });

  it("throttled while deleting hub entries → 503; the retry removes hubs, search row and files", async () => {
    const h = harness();
    await published(h);
    throttleOnce(h, "delete", "HUB#");
    const first = await h.call("POST", "/v1/pages/test-page/withdraw", {
      body: { reason: "wrong figure" },
      key: "withdraw-0002",
    });
    expect(first.status).toBe(503);
    const retry = await h.call("POST", "/v1/pages/test-page/withdraw", {
      body: { reason: "wrong figure" },
      key: "withdraw-0002",
    });
    expect(retry.status).toBe(200);
    expect(items(h, "HUB#")).toEqual([]);
    expect(h.bucket.objects.size).toBe(0);
    expect(h.search.rows.has("test-page")).toBe(false);
    expect(latest(h)).toMatchObject({ status: "withdrawn", withdraw_reason: "wrong figure" });
  });

  it("a failed search removal → the retry still removes the row and the files", async () => {
    const h = harness();
    await published(h);
    const remove = h.search.remove.bind(h.search);
    let failed = false;
    h.search.remove = async (page) => {
      if (!failed) {
        failed = true;
        throw new Error("D1 unavailable (test)");
      }
      return remove(page);
    };
    expect(
      (await h.call("POST", "/v1/pages/test-page/withdraw", { body: { reason: "wrong" }, key: "withdraw-0003" }))
        .status,
    ).toBe(500);
    expect(
      (await h.call("POST", "/v1/pages/test-page/withdraw", { body: { reason: "wrong" }, key: "withdraw-0003" }))
        .status,
    ).toBe(200);
    expect(h.search.rows.has("test-page")).toBe(false);
    expect(h.bucket.objects.size).toBe(0);
  });
});

describe("index promotion is resumable", () => {
  it("a failed purge → the retry purges the page", async () => {
    const h = harness();
    await published(h);
    h.purger.failNext = true;
    const body = { index_status: "index" };
    expect((await h.call("POST", "/v1/pages/test-page/index", { body, key: "index-0001" })).status).toBe(502);
    const retry = await h.call("POST", "/v1/pages/test-page/index", { body, key: "index-0001" });
    expect(retry.status).toBe(200);
    expect(((await retry.json()) as { purged: string[] }).purged).toContain(`${ORIGIN}/2026/11/test-page`);
  });

  it("throttled while updating hub entries → the retry updates every hub entry and the search row", async () => {
    const h = harness();
    await published(h);
    throttleOnce(h, "put", "HUB#");
    const body = { index_status: "index" };
    expect((await h.call("POST", "/v1/pages/test-page/index", { body, key: "index-0002" })).status).toBe(503);
    expect((await h.call("POST", "/v1/pages/test-page/index", { body, key: "index-0002" })).status).toBe(200);
    expect(items(h, "HUB#").every((e) => e.index_status === "index")).toBe(true);
    expect(h.search.rows.get("test-page")?.index_status).toBe("index");
  });

  it("an index request prepared before a withdrawal cannot bring the page back", async () => {
    const h = harness();
    await published(h);
    const stale = latest(h) as never;
    expect((await h.call("POST", "/v1/pages/test-page/withdraw", { body: { reason: "gone" } })).status).toBe(200);
    await expect(executeIndex(h.deps, { page: stale, indexStatus: "index" })).rejects.toMatchObject({
      code: "version_conflict",
    });
    expect(latest(h)).toMatchObject({ status: "withdrawn" });
    expect(items(h, "HUB#")).toEqual([]);
  });
});

describe("publishing retries keep the invalidation set", () => {
  async function update(h: H, key: string) {
    const v2 = makeMeta({ version: 2, categories: ["technology"], entities: [], chart_ids: [] });
    return h.call("PUT", "/v1/pages/test-page", { body: await makeBundle(v2), key });
  }

  it("a failed purge → the retry still purges the hubs and chart file the page left, and deletes the file", async () => {
    const h = harness();
    await published(h);
    h.purger.failNext = true;
    expect((await update(h, "pub-v2-0001")).status).toBe(502);
    const retry = await update(h, "pub-v2-0001");
    expect(retry.status).toBe(200);
    const purged = ((await retry.json()) as { purged: string[] }).purged;
    expect(purged).toEqual(
      expect.arrayContaining([
        `${ORIGIN}/economy/inflation`,
        `${ORIGIN}/economy`,
        `${ORIGIN}/entity/country%3AUS`,
        `${ORIGIN}/2026/11/test-page/charts/cpi-trend.csv`,
      ]),
    );
    expect(h.bucket.objects.has("2026/11/test-page/charts/cpi-trend.csv")).toBe(false);
  });

  it("throttled after the commit → the retry removes the entries of hubs the page left", async () => {
    const h = harness();
    await published(h);
    throttleOnce(h, "delete", "HUB#");
    expect((await update(h, "pub-v2-0002")).status).toBe(503);
    expect((await update(h, "pub-v2-0002")).status).toBe(200);
    expect(items(h, "HUB#cat/economy")).toEqual([]);
    expect(items(h, "HUB#ent/")).toEqual([]);
    expectItemsValid(h.table.dump());
  });

  it("a version conflict found before writing leaves the live files alone", async () => {
    const h = harness();
    await published(h);
    const before = h.bucket.text("2026/11/test-page/index.md");
    // A version record for v2 exists with other content (a publish that never went live), so v2 is refused up front.
    const v2 = await makeBundle(makeMeta({ version: 2, title: "Other content" }));
    h.table.seed({
      ...(latest(h) as Record<string, never>),
      SK: "V#000002",
      version: 2,
      bundle_sha256: "f".repeat(64),
    });
    const res = await h.call("PUT", "/v1/pages/test-page", { body: v2 });
    expect([res.status, await errorCode(res)]).toEqual([409, "version_conflict"]);
    expect(h.bucket.text("2026/11/test-page/index.md")).toBe(before);
  });
});

describe("a publish abandoned before its commit leaves nothing behind", () => {
  /** v2 adds a category, an entity and a chart, writes its files and hub entries, then fails before LATEST moves. */
  async function abandonedV2(h: H) {
    await published(h);
    const v1 = makeMeta();
    const v2 = makeMeta({
      version: 2,
      categories: [...v1.categories, "technology"],
      entities: [...v1.entities, { id: "country:FR", name: "France", kind: "country" }],
      chart_ids: [...v1.chart_ids, "gdp-new"],
    });
    const index = h.search.index.bind(h.search);
    h.search.index = async () => {
      h.search.index = index;
      throw new Error("D1 unavailable (test)");
    };
    expect((await h.call("PUT", "/v1/pages/test-page", { body: await makeBundle(v2) })).status).toBe(500);
    expect(items(h, "HUB#cat/technology")).toHaveLength(1);
    expect(h.bucket.objects.has("2026/11/test-page/charts/gdp-new.csv")).toBe(true);
    expect(latest(h)).toMatchObject({ version: 1 });
  }

  it("v2 fails, v3 leaves out what v2 added: v3 removes v2's hub entries and files and purges them", async () => {
    const h = harness();
    await abandonedV2(h);
    const v3 = await h.call("PUT", "/v1/pages/test-page", { body: await makeBundle(makeMeta({ version: 3 })) });
    expect(v3.status).toBe(200);
    expect(items(h, "HUB#cat/technology")).toEqual([]);
    expect(items(h, "HUB#ent/country:FR")).toEqual([]);
    expect(h.bucket.objects.has("2026/11/test-page/charts/gdp-new.csv")).toBe(false);
    expect(((await v3.json()) as { purged: string[] }).purged).toContain(
      `${ORIGIN}/2026/11/test-page/charts/gdp-new.csv`,
    );
    expectItemsValid(h.table.dump());
    expect((await h.call("POST", "/v1/pages/test-page/withdraw", { body: { reason: "wrong" } })).status).toBe(200);
    expect(items(h, "HUB#")).toEqual([]);
    expect(h.bucket.objects.size).toBe(0);
  });

  it("v2 fails, then the page is withdrawn: v2's hub entries and files go too", async () => {
    const h = harness();
    await abandonedV2(h);
    const res = await h.call("POST", "/v1/pages/test-page/withdraw", { body: { reason: "wrong" } });
    expect(res.status).toBe(200);
    expect(items(h, "HUB#")).toEqual([]);
    expect(h.bucket.objects.size).toBe(0);
    expect(((await res.json()) as { purged: string[] }).purged).toContain(
      `${ORIGIN}/2026/11/test-page/charts/gdp-new.csv`,
    );
  });

  it("a first publish that never committed fixes the page's URL and publication time for the next attempt", async () => {
    const h = harness();
    const index = h.search.index.bind(h.search);
    h.search.index = async () => {
      h.search.index = index;
      throw new Error("D1 unavailable (test)");
    };
    expect((await h.call("PUT", "/v1/pages/test-page", { body: await makeBundle() })).status).toBe(500);
    const moved = makeMeta({ version: 2, path: "/2026/11/elsewhere" });
    const refused = await h.call("PUT", "/v1/pages/test-page", { body: await makeBundle(moved) });
    expect(await errorCode(refused)).toBe("path_conflict");
    const later = makeMeta({ version: 2, published_at: "2026-11-12T09:00:00Z", updated_at: "2026-11-12T09:00:00Z" });
    expect((await h.call("PUT", "/v1/pages/test-page", { body: await makeBundle(later) })).status).toBe(200);
    // One entry per hub, at the first attempt's time: no duplicate left at the abandoned attempt's sort key.
    expect(items(h, "HUB#latest")).toHaveLength(1);
    expect(latest(h)?.published_at).toBe(makeMeta().published_at.replace("Z", ".000Z"));
  });
});

describe("a withdrawn page stays withdrawn", () => {
  it("re-sending the withdrawn version → 409 page_withdrawn; a higher version publishes again", async () => {
    const h = harness();
    const v1 = await makeBundle();
    expect((await h.call("PUT", "/v1/pages/test-page", { body: v1, key: "pub-v1-0001" })).status).toBe(200);
    expect((await h.call("POST", "/v1/pages/test-page/withdraw", { body: { reason: "wrong figure" } })).status).toBe(
      200,
    );
    const again = await h.call("PUT", "/v1/pages/test-page", { body: v1, key: "pub-v1-0001-later" });
    expect([again.status, await errorCode(again)]).toEqual([409, "page_withdrawn"]);
    expect(latest(h)).toMatchObject({ status: "withdrawn" });
    expect(h.bucket.objects.size).toBe(0);
    const v2 = await makeBundle(makeMeta({ version: 2 }));
    expect((await h.call("PUT", "/v1/pages/test-page", { body: v2 })).status).toBe(200);
    expect(latest(h)).toMatchObject({ status: "published", version: 2 });
    expect(latest(h)?.withdrawn_at).toBeUndefined();
  });
});

describe("the search shard is checked for the date the record keeps", () => {
  it("a 2025 page republished with a 2026 date needs the 2025 shard, and nothing is written without it", async () => {
    const h = harness();
    const v1 = makeMeta({ published_at: "2025-12-30T12:00:00Z", updated_at: "2025-12-30T12:00:00Z" });
    expect((await h.call("PUT", "/v1/pages/test-page", { body: await makeBundle(v1) })).status).toBe(200);
    h.search.years = new Set([2026]);
    const [r2Before, searchBefore, tableBefore] = [
      h.bucket.writes.length,
      h.search.calls.length,
      h.table.writes.length,
    ];
    const v2 = makeMeta({ version: 2, published_at: "2026-11-11T12:00:00Z", updated_at: "2026-11-11T12:00:00Z" });
    const res = await h.call("PUT", "/v1/pages/test-page", { body: await makeBundle(v2) });
    expect([res.status, await errorCode(res)]).toEqual([500, "not_configured"]);
    expect([h.bucket.writes.length, h.search.calls.length]).toEqual([r2Before, searchBefore]);
    // Only the page lock is taken and given back; no content item is written.
    const tableWrites = h.table.writes.slice(tableBefore).flatMap((c) => c.keys.map((k) => `${k.PK} ${k.SK}`));
    expect(tableWrites.every((k) => k === "PAGE#test-page LOCK")).toBe(true);
    expect(items(h, "PAGE#test-page").map((i) => i.SK)).toEqual(["LATEST", "V#000001"]);
  });
});

describe("home curation retries never publish a second version", () => {
  const curationKeys = (h: H) =>
    h.table
      .dump()
      .filter((i) => i.PK === "HOME#CURATION")
      .map((i) => i.SK);

  it("a failed purge → the retry purges and answers with the same version", async () => {
    const h = harness();
    h.table.seed(...(curationPages() as never[]));
    h.purger.failNext = true;
    expect((await h.call("PUT", "/v1/home/curation", { body: makeCuration(), key: "curation-0001" })).status).toBe(502);
    const retry = await h.call("PUT", "/v1/home/curation", { body: makeCuration(), key: "curation-0001" });
    expect(retry.status).toBe(200);
    expect(await retry.json()).toMatchObject({ version: 1, live_version: 1 });
    expect(curationKeys(h)).toEqual(["LATEST", "V#000001"]);
  });

  it("a lost response from the commit keeps the key; the retry finds the committed version", async () => {
    const h = harness();
    h.table.seed(...(curationPages() as never[]));
    const transact = h.table.transact.bind(h.table);
    let lose = true;
    h.table.transact = async (ops) => {
      await transact(ops);
      if (lose) {
        lose = false;
        throw new OutcomeUnknownError("timed out (test)");
      }
    };
    const first = await h.call("PUT", "/v1/home/curation", { body: makeCuration(), key: "curation-0002" });
    expect([first.status, await errorCode(first)]).toEqual([503, "outcome_unknown"]);
    const idem = h.table.dump().find((i) => String(i.PK).startsWith("IDEM#"));
    expect(idem).toMatchObject({ state: "pending" });
    // Straight away the key is still held; once the lock is stale the retry resumes instead of publishing again.
    const busy = await h.call("PUT", "/v1/home/curation", { body: makeCuration(), key: "curation-0002" });
    expect(await errorCode(busy)).toBe("idempotency_in_progress");
    h.setNow("2026-11-12T10:05:00Z");
    const retry = await h.call("PUT", "/v1/home/curation", { body: makeCuration(), key: "curation-0002" });
    expect(retry.status).toBe(200);
    expect(await retry.json()).toMatchObject({ version: 1, live_version: 1 });
    expect(curationKeys(h)).toEqual(["LATEST", "V#000001"]);
  });

  it("a curation whose replay record was never written is not published twice after the lock goes stale", async () => {
    const h = harness();
    h.table.seed(...(curationPages() as never[]));
    const put = h.table.put.bind(h.table);
    h.table.put = async (item, cond) => {
      if (String(item.PK).startsWith("IDEM#") && item.state === "done") throw new Error("lost (test)");
      return put(item, cond);
    };
    expect((await h.call("PUT", "/v1/home/curation", { body: makeCuration(), key: "curation-0003" })).status).toBe(200);
    h.table.put = put;
    h.setNow("2026-11-12T10:05:00Z");
    const retry = await h.call("PUT", "/v1/home/curation", { body: makeCuration(), key: "curation-0003" });
    expect(await retry.json()).toMatchObject({ version: 1 });
    expect(curationKeys(h)).toEqual(["LATEST", "V#000001"]);
  });
});

describe("one page, one writer at a time", () => {
  it("a publish that starts while another is writing the same page waits its turn (409 page_busy)", async () => {
    const h = harness();
    await published(h);
    const v2 = await makeBundle(makeMeta({ version: 2, title: "Version two" }));
    const v3 = await makeBundle(makeMeta({ version: 3, title: "Version three" }));
    let inner: Response | undefined;
    const bucketPut = h.bucket.put.bind(h.bucket);
    h.bucket.put = async (key, value, options) => {
      if (!inner) inner = await h.call("PUT", "/v1/pages/test-page", { body: v3, key: "pub-v3-0001" });
      return bucketPut(key, value, options);
    };
    const outer = await h.call("PUT", "/v1/pages/test-page", { body: v2, key: "pub-v2-0001" });
    expect(outer.status).toBe(200);
    expect([inner?.status, inner && (await errorCode(inner))]).toEqual([409, "page_busy"]);
    h.bucket.put = bucketPut;
    expect((await h.call("PUT", "/v1/pages/test-page", { body: v3, key: "pub-v3-0001" })).status).toBe(200);
    expect(latest(h)).toMatchObject({ version: 3, title: "Version three" });
    expect(h.bucket.text("2026/11/test-page/index.md")).toContain("Version three");
    expect(items(h, "HUB#latest")[0]).toMatchObject({ title: "Version three" });
    expect(items(h, "PAGE#test-page").map((i) => i.SK)).not.toContain("LOCK");
  });

  it("a withdrawal that starts while an index promotion is running is refused, so the two never interleave", async () => {
    const h = harness();
    await published(h);
    let inner: Response | undefined;
    const setStatus = h.search.setIndexStatus.bind(h.search);
    h.search.setIndexStatus = async (page, status) => {
      inner = await h.call("POST", "/v1/pages/test-page/withdraw", { body: { reason: "gone" } });
      return setStatus(page, status);
    };
    expect((await h.call("POST", "/v1/pages/test-page/index", { body: { index_status: "index" } })).status).toBe(200);
    expect(inner && (await errorCode(inner))).toBe("page_busy");
    expect(latest(h)).toMatchObject({ status: "published", index_status: "index" });
  });

  it("a failed file write waits for every write still in flight before the lock is given back", async () => {
    const h = harness();
    await published(h);
    const bucketPut = h.bucket.put.bind(h.bucket);
    let letGo = () => {};
    const held = new Promise<void>((r) => {
      letGo = r;
    });
    let slowLanded = false;
    let failed = false;
    h.bucket.put = async (key, value, options) => {
      if (key.endsWith("/index.html")) {
        await held;
        const out = await bucketPut(key, value, options);
        slowLanded = true;
        return out;
      }
      if (key.endsWith("/index.md")) {
        failed = true;
        throw new Error("R2 unavailable (test)");
      }
      return bucketPut(key, value, options);
    };
    const v2 = await makeBundle(makeMeta({ version: 2, title: "Version two" }));
    let settled = false;
    const outer = h.call("PUT", "/v1/pages/test-page", { body: v2 }).then((r) => {
      settled = true;
      return r;
    });
    while (!failed) await new Promise((r) => setTimeout(r, 1));
    for (let i = 0; i < 10; i++) await new Promise((r) => setTimeout(r, 1));
    // index.md failed, but index.html is still being written: the request has not answered and still holds the lock.
    expect(settled).toBe(false);
    const v3 = await makeBundle(makeMeta({ version: 3, title: "Version three" }));
    expect(await errorCode(await h.call("PUT", "/v1/pages/test-page", { body: v3 }))).toBe("page_busy");
    letGo();
    const res = await outer;
    expect([res.status, slowLanded]).toEqual([500, true]);
    expect(items(h, "PAGE#test-page").map((i) => i.SK)).not.toContain("LOCK");
  }, 20_000);

  it("a lock left by a crashed request is taken over once it is stale", async () => {
    const h = harness();
    await published(h);
    h.table.seed({
      PK: "PAGE#test-page",
      SK: "LOCK",
      type: "page_lock",
      owner: "crashed",
      acquired_at: "2026-11-12T09:59:00.000Z",
      expires_at: 1_900_000_000,
    });
    const v2 = await makeBundle(makeMeta({ version: 2 }));
    expect(await errorCode(await h.call("PUT", "/v1/pages/test-page", { body: v2 }))).toBe("page_busy");
    h.setNow("2026-11-12T10:02:00Z");
    expect((await h.call("PUT", "/v1/pages/test-page", { body: v2 })).status).toBe(200);
  });
});

describe("one request stays inside the free plan's 50 external subrequests", () => {
  // The schema's maximum: 8 categories four levels deep (32 hubs), 50 entities, latest and region: 84 hubs; 64 files.
  const maxMeta = (version: number, tag: string) =>
    makeMeta({
      version,
      categories: Array.from({ length: 8 }, (_, i) => `${tag}${i}/a/b/c`),
      entities: Array.from({ length: 50 }, (_, i) => ({ id: `company:${tag}${i}`, name: `C${i}`, kind: "company" })),
      chart_ids: Array.from({ length: 58 }, (_, i) => `${tag}-chart-${i}`),
    });

  /** DynamoDB calls plus purge API calls (30 URLs each): every external subrequest the request made. */
  async function subrequests(h: H, run: () => Promise<Response>) {
    const [calls, purges] = [h.table.calls.length, h.purger.calls.length];
    const res = await run();
    expect(res.status).toBe(200);
    const purgeCalls = h.purger.calls.slice(purges).reduce((n, urls) => n + Math.ceil(urls.length / 30), 0);
    return h.table.calls.length - calls + purgeCalls;
  }

  it("publishing, replacing every hub and entity, and withdrawing at the schema maximum", async () => {
    const h = harness();
    const v1 = await makeBundle(maxMeta(1, "x"));
    expect(v1.manifest.files).toHaveLength(64);
    expect(await subrequests(h, () => h.call("PUT", "/v1/pages/test-page", { body: v1 }))).toBeLessThanOrEqual(45);
    expect(items(h, "HUB#")).toHaveLength(84);
    const v2 = await makeBundle(maxMeta(2, "y"));
    expect(await subrequests(h, () => h.call("PUT", "/v1/pages/test-page", { body: v2 }))).toBeLessThanOrEqual(45);
    expect(items(h, "HUB#")).toHaveLength(84);
    expect(items(h, "ENT#")).toHaveLength(100);
    const withdraw = () => h.call("POST", "/v1/pages/test-page/withdraw", { body: { reason: "wrong" } });
    expect(await subrequests(h, withdraw)).toBeLessThanOrEqual(45);
    expect(items(h, "HUB#")).toEqual([]);
  });
});

describe("idempotency records are bookkeeping only", () => {
  it("the key hash, never the key, is stored", async () => {
    const h = harness();
    await h.call("PUT", "/v1/pages/test-page", { body: await makeBundle(), key: "secret-looking-key-1" });
    const dump = JSON.stringify(h.table.dump());
    expect(dump).not.toContain("secret-looking-key-1");
    expect(dump).toContain(await sha256Hex("secret-looking-key-1"));
  });
});
