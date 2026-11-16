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

/** Makes the first call of `method` on the table whose key starts with `prefix` fail as throttled. */
function throttleOnce(h: H, method: "put" | "delete", prefix: string) {
  const original = h.table[method].bind(h.table) as (a: never, c?: never) => Promise<void>;
  let done = false;
  (h.table as unknown as Record<string, unknown>)[method] = async (arg: { PK: string }, cond?: never) => {
    if (!done && String(arg.PK).startsWith(prefix)) {
      done = true;
      h.table.throttleNext = 1;
    }
    return original(arg as never, cond);
  };
}

describe("withdrawal is resumable", () => {
  it("a failed purge → 502; the retry with the same key purges and returns 200", async () => {
    const h = harness();
    await published(h);
    h.purger.failNext = true;
    const first = await h.call("POST", "/v1/pages/test-page/withdraw", { body: { reason: "wrong figure" }, key: "withdraw-0001" });
    expect([first.status, await errorCode(first)]).toEqual([502, "purge_failed"]);
    const retry = await h.call("POST", "/v1/pages/test-page/withdraw", { body: { reason: "wrong figure" }, key: "withdraw-0001" });
    expect(retry.status).toBe(200);
    expect(((await retry.json()) as { purged: string[] }).purged).toContain(`${ORIGIN}/2026/11/test-page`);
  });

  it("throttled while deleting hub entries → 503; the retry removes hubs, search row and files", async () => {
    const h = harness();
    await published(h);
    throttleOnce(h, "delete", "HUB#");
    const first = await h.call("POST", "/v1/pages/test-page/withdraw", { body: { reason: "wrong figure" }, key: "withdraw-0002" });
    expect(first.status).toBe(503);
    const retry = await h.call("POST", "/v1/pages/test-page/withdraw", { body: { reason: "wrong figure" }, key: "withdraw-0002" });
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
    expect((await h.call("POST", "/v1/pages/test-page/withdraw", { body: { reason: "wrong" }, key: "withdraw-0003" })).status).toBe(500);
    expect((await h.call("POST", "/v1/pages/test-page/withdraw", { body: { reason: "wrong" }, key: "withdraw-0003" })).status).toBe(200);
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
    h.table.seed({ ...(latest(h) as Record<string, never>), SK: "V#000002", version: 2, bundle_sha256: "f".repeat(64) });
    const res = await h.call("PUT", "/v1/pages/test-page", { body: v2 });
    expect([res.status, await errorCode(res)]).toEqual([409, "version_conflict"]);
    expect(h.bucket.text("2026/11/test-page/index.md")).toBe(before);
  });
});

describe("a withdrawn page stays withdrawn", () => {
  it("re-sending the withdrawn version → 409 page_withdrawn; a higher version publishes again", async () => {
    const h = harness();
    const v1 = await makeBundle();
    expect((await h.call("PUT", "/v1/pages/test-page", { body: v1, key: "pub-v1-0001" })).status).toBe(200);
    expect((await h.call("POST", "/v1/pages/test-page/withdraw", { body: { reason: "wrong figure" } })).status).toBe(200);
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
    const [r2Before, searchBefore, tableBefore] = [h.bucket.writes.length, h.search.calls.length, h.table.writes.length];
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

describe("idempotency records are bookkeeping only", () => {
  it("the key hash, never the key, is stored", async () => {
    const h = harness();
    await h.call("PUT", "/v1/pages/test-page", { body: await makeBundle(), key: "secret-looking-key-1" });
    const dump = JSON.stringify(h.table.dump());
    expect(dump).not.toContain("secret-looking-key-1");
    expect(dump).toContain(await sha256Hex("secret-looking-key-1"));
  });
});
