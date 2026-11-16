// Unit tests (inside workerd): publishing, idempotency, conflicts, withdrawal and index promotion.

import { type Bundle, makeBundle, makeMeta } from "@news/worker-shared/testing/fixtures";
import { schemas } from "@news/worker-shared/validate";
import { describe, expect, it } from "vitest";
import { expectItemsValid, harness, ORIGIN } from "./testing";

const put = (h: ReturnType<typeof harness>, bundle: Bundle, key?: string) =>
  h.call("PUT", `/v1/pages/${bundle.manifest.page_id}`, { body: bundle, key });

describe("PUT /v1/pages/{id}", () => {
  it("writes R2 files, PAGE, HUB, ENT items and the search row, then purges the affected URLs", async () => {
    const h = harness();
    const bundle = await makeBundle();
    const res = await put(h, bundle);
    expect(res.status).toBe(200);
    const body = (await res.json()) as { purged: string[] };
    expect(schemas.validatePutPageResponse(body)).toBe(true);
    expect(body).toMatchObject({
      page_id: "test-page",
      version: 1,
      url: `${ORIGIN}/2026/11/test-page`,
      status: "published",
    });

    expect([...h.bucket.objects.keys()].sort()).toEqual(
      Object.keys(bundle.files)
        .map((f) => `2026/11/test-page/${f}`)
        .sort(),
    );
    expect(h.bucket.objects.get("2026/11/test-page/index.html")?.contentType).toBe("text/html; charset=utf-8");
    expect(h.bucket.text("2026/11/test-page/data.json")).toBe(bundle.files["data.json"]);

    const items = h.table.dump();
    expectItemsValid(items);
    const keys = items.map((i) => `${i.PK} ${i.SK}`);
    expect(keys).toEqual(
      expect.arrayContaining([
        "PAGE#test-page LATEST",
        "PAGE#test-page V#000001",
        "HUB#latest 2026-11-10T12:00:00.000Z#test-page",
        "HUB#cat/economy 2026-11-10T12:00:00.000Z#test-page",
        "HUB#cat/economy/inflation 2026-11-10T12:00:00.000Z#test-page",
        "HUB#ent/country:US 2026-11-10T12:00:00.000Z#test-page",
        "HUB#region/north-america 2026-11-10T12:00:00.000Z#test-page",
        "ENT#country:US META",
        "URL#/2026/11/test-page PAGE",
      ]),
    );
    const latest = items.find((i) => i.PK === "PAGE#test-page" && i.SK === "LATEST");
    expect(latest).toMatchObject({
      lang: "en",
      status: "published",
      version: 1,
      bundle_sha256: bundle.manifest.bundle_sha256,
    });
    expect(JSON.stringify(latest)).not.toContain("root = Page"); // page.oui stays in R2

    expect(h.search.calls).toEqual([{ op: "index", page_id: "test-page" }]);
    expect(h.purger.all).toEqual(
      expect.arrayContaining([
        `${ORIGIN}/2026/11/test-page`,
        `${ORIGIN}/2026/11/test-page/index.md`,
        `${ORIGIN}/latest`,
        `${ORIGIN}/economy/inflation`,
        `${ORIGIN}/entity/country%3AUS`,
        `${ORIGIN}/region/north-america`,
        `${ORIGIN}/`,
      ]),
    );
    expect(body.purged).toEqual(h.purger.all);
  });

  it("replaying the same Idempotency-Key and body returns the first response unchanged and writes nothing", async () => {
    const h = harness();
    const bundle = await makeBundle();
    const first = await put(h, bundle, "publish:test-page:1:abc");
    const firstText = await first.text();
    const before = h.writeCount();
    const tableBefore = JSON.stringify(h.table.dump());

    for (let i = 0; i < 3; i++) {
      const again = await put(h, bundle, "publish:test-page:1:abc");
      expect(again.status).toBe(first.status);
      expect(await again.text()).toBe(firstText);
      expect(again.headers.get("idempotent-replayed")).toBe("true");
    }
    expect(h.writeCount()).toBe(before);
    expect(JSON.stringify(h.table.dump())).toBe(tableBefore);
  });

  it("the same key with a different body → 409 and nothing written", async () => {
    const h = harness();
    await put(h, await makeBundle(), "publish:test-page:1:abc");
    const before = h.writeCount();
    const other = await makeBundle(makeMeta({ title: "A different title" }));
    const res = await put(h, other, "publish:test-page:1:abc");
    expect(res.status).toBe(409);
    expect(((await res.json()) as { error: { code: string } }).error.code).toBe("idempotency_key_reused");
    expect(h.writeCount()).toBe(before);
  });

  it("a key still being processed → 409 in progress; a stale lock is taken over", async () => {
    const h = harness();
    const bundle = await makeBundle();
    const { requestHash } = await import("./idempotency");
    const { sha256Hex } = await import("@news/worker-shared/crypto");
    const rh = await requestHash("PUT", "/v1/pages/test-page", JSON.stringify(bundle));
    h.table.seed({
      PK: `IDEM#${await sha256Hex("busy-key-123")}`,
      SK: "META",
      type: "idempotency",
      request_hash: rh,
      state: "pending",
      operation: "put_page",
      created_at: "2026-11-12T09:59:30.000Z",
      expires_at: 1_900_000_000,
    });
    const busy = await put(h, bundle, "busy-key-123");
    expect(busy.status).toBe(409);
    expect(((await busy.json()) as { error: { code: string } }).error.code).toBe("idempotency_in_progress");
    h.setNow("2026-11-12T10:05:00Z");
    expect((await put(h, bundle, "busy-key-123")).status).toBe(200);
  });

  it("a failure releases the key: throttling → 503, and the retry with the same key succeeds", async () => {
    const h = harness();
    const bundle = await makeBundle();
    const original = h.table.batchWrite.bind(h.table);
    let failed = false;
    h.table.batchWrite = async (ops) => {
      if (!failed) {
        failed = true;
        h.table.throttleNext = 1;
      }
      return original(ops);
    };
    const res = await put(h, bundle, "retry-key-123");
    expect(res.status).toBe(503);
    expect(res.headers.get("retry-after")).toBe("2");
    expect(h.table.dump().some((i) => String(i.PK).startsWith("IDEM#"))).toBe(false);
    const retry = await put(h, bundle, "retry-key-123");
    expect(retry.status).toBe(200);
    expectItemsValid(h.table.dump());
  });

  it("a failed purge → 502 purge_failed; the retry with the same key writes again and purges", async () => {
    const h = harness();
    const bundle = await makeBundle();
    h.purger.failNext = true;
    const res = await put(h, bundle, "purge-key-123");
    expect(res.status).toBe(502);
    expect(((await res.json()) as { error: { code: string } }).error.code).toBe("purge_failed");
    expect((await put(h, bundle, "purge-key-123")).status).toBe(200);
    expect(h.purger.calls).toHaveLength(1);
  });

  it("refuses an older version, a different bundle under the live version, and a URL change", async () => {
    const h = harness();
    await put(h, await makeBundle(makeMeta({ version: 2 })));
    const code = async (r: Response) => [r.status, ((await r.json()) as { error: { code: string } }).error.code];
    expect(await code(await put(h, await makeBundle(makeMeta({ version: 1 }))))).toEqual([409, "version_conflict"]);
    expect(await code(await put(h, await makeBundle(makeMeta({ version: 2, title: "changed" }))))).toEqual([
      409,
      "version_conflict",
    ]);
    expect(await code(await put(h, await makeBundle(makeMeta({ version: 3, path: "/2026/12/test-page" }))))).toEqual([
      409,
      "path_conflict",
    ]);
    // Re-sending the live version unchanged (a new key) is fine.
    expect((await put(h, await makeBundle(makeMeta({ version: 2 })))).status).toBe(200);
  });

  it("stores timestamps in one form (so they compare and sort as times) and refuses days that do not exist", async () => {
    const h = harness();
    const meta = makeMeta({ published_at: "2026-11-15T12:00:00Z", updated_at: "2026-11-15T12:00:00.001Z" });
    expect((await put(h, await makeBundle(meta))).status).toBe(200);
    const keys = h.table.dump().map((i) => `${i.PK} ${i.SK}`);
    expect(keys).toContain("HUB#latest 2026-11-15T12:00:00.000Z#test-page");
    expect(h.table.dump().find((i) => i.SK === "LATEST")).toMatchObject({
      published_at: "2026-11-15T12:00:00.000Z",
      updated_at: "2026-11-15T12:00:00.001Z",
    });
    for (const bad of ["2026-02-31T12:00:00Z", "2026-02-29T12:00:00Z"]) {
      const res = await put(h, await makeBundle(makeMeta({ id: "other", published_at: bad, updated_at: bad })));
      expect(res.status).toBe(400);
      expect(((await res.json()) as { error: { code: string } }).error.code).toBe("invalid_meta");
    }
  });

  it("refuses a page that would take another page's URL", async () => {
    const h = harness();
    await put(h, await makeBundle(makeMeta({ id: "first", path: "/2026/11/shared-path" })));
    const res = await put(h, await makeBundle(makeMeta({ id: "second", path: "/2026/11/shared-path" })));
    expect(res.status).toBe(409);
    expect(((await res.json()) as { error: { code: string } }).error.code).toBe("path_conflict");
    expect(h.table.dump().find((i) => i.PK === "PAGE#second")).toBeUndefined();
  });

  it("refuses a year with no search shard before writing anything", async () => {
    const h = harness();
    h.search.years = new Set([2025]);
    const res = await put(h, await makeBundle());
    expect(res.status).toBe(500);
    expect(((await res.json()) as { error: { code: string } }).error.code).toBe("not_configured");
    // Only the page lock was taken and given back.
    expect(h.table.writes.flatMap((c) => c.keys.map((k) => k.SK))).toEqual(["LOCK", "LOCK"]);
    expect(h.writeCount()).toBe(2);
  });

  it("an update keeps the hub position, drops hubs and chart files the page left, keeps old versions", async () => {
    const h = harness();
    await put(h, await makeBundle());
    const v2 = makeMeta({
      version: 2,
      categories: ["technology"],
      entities: [],
      chart_ids: [],
      updated_at: "2026-11-11T08:00:00Z",
      published_at: "2026-11-11T08:00:00Z",
    });
    expect((await put(h, await makeBundle(v2))).status).toBe(200);
    const keys = h.table.dump().map((i) => `${i.PK} ${i.SK}`);
    expect(keys).toContain("HUB#cat/technology 2026-11-10T12:00:00.000Z#test-page");
    expect(keys).toContain("HUB#latest 2026-11-10T12:00:00.000Z#test-page");
    expect(keys.filter((k) => k.startsWith("HUB#cat/economy"))).toEqual([]);
    expect(keys).not.toContain("HUB#ent/country:US 2026-11-10T12:00:00.000Z#test-page");
    expect(keys).toEqual(expect.arrayContaining(["PAGE#test-page V#000001", "PAGE#test-page V#000002"]));
    expect(h.bucket.objects.has("2026/11/test-page/charts/cpi-trend.csv")).toBe(false);
    expect(h.purger.calls[1]).toContain(`${ORIGIN}/economy/inflation`);
    expectItemsValid(h.table.dump());
  });
});

describe("POST /v1/pages/{id}/withdraw", () => {
  it("marks the page withdrawn, removes hub entries, search row and public files, and purges", async () => {
    const h = harness();
    await put(h, await makeBundle());
    const res = await h.call("POST", "/v1/pages/test-page/withdraw", { body: { reason: "Wrong source figure" } });
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(schemas.validatePageOpResponse(body)).toBe(true);
    expect(body).toMatchObject({ status: "withdrawn" });
    const items = h.table.dump();
    expectItemsValid(items);
    expect(items.find((i) => i.PK === "PAGE#test-page" && i.SK === "LATEST")).toMatchObject({
      status: "withdrawn",
      withdraw_reason: "Wrong source figure",
    });
    expect(items.filter((i) => String(i.PK).startsWith("HUB#"))).toEqual([]);
    expect(h.bucket.objects.size).toBe(0);
    expect(h.search.rows.has("test-page")).toBe(false);
    expect(h.purger.calls[1]).toEqual(expect.arrayContaining([`${ORIGIN}/2026/11/test-page`, `${ORIGIN}/`]));
    // Withdrawing again keeps the first reason and time; it only repeats the (idempotent) clean-up and purge.
    const before = JSON.stringify(h.table.dump());
    expect((await h.call("POST", "/v1/pages/test-page/withdraw", { body: { reason: "again" } })).status).toBe(200);
    const content = (dump: string) =>
      (JSON.parse(dump) as { PK: string }[]).filter((i) => !i.PK.startsWith("IDEM#")).map((i) => JSON.stringify(i));
    expect(content(JSON.stringify(h.table.dump()))).toEqual(content(before));
  });

  it("404 for an unknown page; 400 without a reason", async () => {
    const h = harness();
    expect((await h.call("POST", "/v1/pages/nope/withdraw", { body: { reason: "x y z" } })).status).toBe(404);
    expect((await h.call("POST", "/v1/pages/nope/withdraw", { body: {} })).status).toBe(400);
  });
});

describe("POST /v1/pages/{id}/index", () => {
  it("promotes the page in its record, hub entries and search row", async () => {
    const h = harness();
    await put(h, await makeBundle());
    const res = await h.call("POST", "/v1/pages/test-page/index", { body: { index_status: "index" } });
    expect(res.status).toBe(200);
    expect(await res.json()).toMatchObject({ index_status: "index", status: "published" });
    const items = h.table.dump();
    expect(
      items
        .filter((i) => i.type === "hub_entry" || i.type === "page")
        .every((i) => i.SK === "V#000001" || i.index_status === "index"),
    ).toBe(true);
    expect(h.search.rows.get("test-page")?.index_status).toBe("index");
  });

  it("refuses a withdrawn page", async () => {
    const h = harness();
    await put(h, await makeBundle());
    await h.call("POST", "/v1/pages/test-page/withdraw", { body: { reason: "gone now" } });
    const res = await h.call("POST", "/v1/pages/test-page/index", { body: { index_status: "index" } });
    expect(res.status).toBe(409);
    expect(((await res.json()) as { error: { code: string } }).error.code).toBe("page_withdrawn");
  });
});
