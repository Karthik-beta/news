// One behaviour suite for every Table: the in-memory fake (shared unit tests) and DynamoTable against DynamoDB Local
// (api integration tests). If the fake ever behaves differently from DynamoDB, one of the two runs fails.
import { describe, expect, it } from "vitest";
import { and, eq, le, notExists, or } from "./conditions";
import { ConditionFailedError, type Doc, type Table } from "./table";

export function tableContract(label: string, getTable: () => Table): void {
  const uid = () => crypto.randomUUID().slice(0, 8);
  const failsWith = async (p: Promise<unknown>, type: new (...args: never[]) => Error) => {
    await expect(p).rejects.toBeInstanceOf(type);
  };

  describe(`Table contract: ${label}`, () => {
    it("round-trips every value type unchanged", async () => {
      const t = getTable();
      const item: Doc = {
        PK: `PAGE#${uid()}`,
        SK: "LATEST",
        s: 'quotes " and — unicode café 📈',
        n: 42,
        f: -3.25,
        b: false,
        z: null,
        l: ["a", 1, true, null, { x: "y" }],
        m: { nested: { deep: [1, 2] }, empty: [] },
      };
      await t.put(item);
      expect(await t.get({ PK: item.PK as string, SK: "LATEST" })).toEqual(item);
    });

    it("get returns undefined for a missing item", async () => {
      expect(await getTable().get({ PK: `PAGE#${uid()}`, SK: "LATEST" })).toBeUndefined();
    });

    it("put replaces the whole item", async () => {
      const t = getTable();
      const key = { PK: `PAGE#${uid()}`, SK: "LATEST" };
      await t.put({ ...key, a: 1, old: "x" });
      await t.put({ ...key, a: 2 });
      expect(await t.get(key)).toEqual({ ...key, a: 2 });
    });

    it("create-only put fails on the second write and keeps the first", async () => {
      const t = getTable();
      const key = { PK: `PAGE#${uid()}`, SK: "V#000001" };
      await t.put({ ...key, v: "first" }, notExists());
      await failsWith(t.put({ ...key, v: "second" }, notExists()), ConditionFailedError);
      expect((await t.get(key))?.v).toBe("first");
    });

    it("'not exists OR same hash' lets an identical retry through and stops a different one", async () => {
      const t = getTable();
      const key = { PK: `PAGE#${uid()}`, SK: "V#000001" };
      const cond = (sha: string) => or(notExists(), eq("sha", sha));
      await t.put({ ...key, sha: "aaa" }, cond("aaa"));
      await t.put({ ...key, sha: "aaa" }, cond("aaa"));
      await failsWith(t.put({ ...key, sha: "bbb" }, cond("bbb")), ConditionFailedError);
    });

    it("'not exists OR version <= n' moves a pointer forward only", async () => {
      const t = getTable();
      const key = { PK: `PAGE#${uid()}`, SK: "LATEST" };
      const move = (v: number) => t.put({ ...key, version: v }, or(notExists(), le("version", v)));
      await move(2);
      await move(3);
      await move(3);
      await failsWith(move(1), ConditionFailedError);
      expect((await t.get(key))?.version).toBe(3);
    });

    it("a comparison on a missing attribute is false", async () => {
      const t = getTable();
      const key = { PK: `PAGE#${uid()}`, SK: "LATEST" };
      await t.put({ ...key });
      await failsWith(t.put({ ...key, x: 1 }, le("version", 5)), ConditionFailedError);
      await failsWith(t.put({ ...key, x: 1 }, eq("state", "done")), ConditionFailedError);
    });

    it("conditional delete", async () => {
      const t = getTable();
      const key = { PK: `IDEM#${uid()}`, SK: "META" };
      await t.put({ ...key, state: "pending" });
      await failsWith(t.delete(key, eq("state", "done")), ConditionFailedError);
      await t.delete(key, and(eq("state", "pending")));
      expect(await t.get(key)).toBeUndefined();
      await t.delete(key);
    });

    it("query: sort-key order, reverse, prefix, between, paging", async () => {
      const t = getTable();
      const pk = `HUB#${uid()}`;
      const sks = ["2026-11-01T00:00:00Z#a", "2026-11-02T00:00:00Z#b", "2026-11-03T00:00:00Z#c", "META"];
      for (const sk of sks) await t.put({ PK: pk, SK: sk });
      const all = await t.query({ pk });
      expect(all.items.map((i) => i.SK)).toEqual(sks);
      expect(all.next).toBeUndefined();
      const newest = await t.query({ pk, sk: { beginsWith: "2026-" }, forward: false, limit: 2 });
      expect(newest.items.map((i) => i.SK)).toEqual([sks[2], sks[1]]);
      expect(newest.next).toBeDefined();
      const rest = await t.query({ pk, sk: { beginsWith: "2026-" }, forward: false, limit: 2, start: newest.next });
      expect(rest.items.map((i) => i.SK)).toEqual([sks[0]]);
      expect(rest.next).toBeUndefined();
      const range = await t.query({ pk, sk: { between: ["2026-11-02", "2026-11-03￿"] } });
      expect(range.items.map((i) => i.SK)).toEqual([sks[1], sks[2]]);
      expect((await t.query({ pk, sk: { eq: "META" } })).items).toHaveLength(1);
      expect((await t.query({ pk: `HUB#${uid()}` })).items).toEqual([]);
    });

    it("zero-padded versions sort numerically; newest first with limit 1", async () => {
      const t = getTable();
      const pk = `PAGE#${uid()}`;
      for (const v of [1, 2, 10, 9]) await t.put({ PK: pk, SK: `V#${String(v).padStart(6, "0")}` });
      const top = await t.query({ pk, sk: { beginsWith: "V#" }, forward: false, limit: 1 });
      expect(top.items[0]?.SK).toBe("V#000010");
    });

    it("batchGet returns the existing items, ignores missing and duplicate keys, handles more than 100", async () => {
      const t = getTable();
      const pk = `PAGE#${uid()}`;
      const keys = Array.from({ length: 120 }, (_, i) => ({ PK: `${pk}-${i}`, SK: "LATEST" }));
      for (const k of keys.slice(0, 110)) await t.put({ ...k, n: Number(k.PK.split("-").pop()) });
      const got = await t.batchGet([...keys, keys[0] as (typeof keys)[0]]);
      expect(got).toHaveLength(110);
      expect(new Set(got.map((d) => d.PK)).size).toBe(110);
    });

    it("transact: all or nothing", async () => {
      const t = getTable();
      const pk = `HOME#${uid()}`;
      await t.put({ PK: pk, SK: "LATEST", version: 1 });
      await failsWith(
        t.transact([
          { put: { PK: pk, SK: "V#000002", version: 2 }, cond: notExists() },
          { put: { PK: pk, SK: "LATEST", version: 2 }, cond: eq("version", 7) },
        ]),
        ConditionFailedError,
      );
      expect(await t.get({ PK: pk, SK: "V#000002" })).toBeUndefined();
      expect((await t.get({ PK: pk, SK: "LATEST" }))?.version).toBe(1);
      await t.put({ PK: pk, SK: "LOCK", held: true });
      await t.transact([
        { put: { PK: pk, SK: "V#000002", version: 2 }, cond: notExists() },
        { put: { PK: pk, SK: "LATEST", version: 2 }, cond: eq("version", 1) },
        { check: { PK: pk, SK: "LOCK" }, cond: eq("held", true) },
      ]);
      expect((await t.get({ PK: pk, SK: "LATEST" }))?.version).toBe(2);
      await failsWith(
        t.transact([{ put: { PK: pk, SK: "V#000003" } }, { check: { PK: pk, SK: "LOCK" }, cond: eq("held", false) }]),
        ConditionFailedError,
      );
      expect(await t.get({ PK: pk, SK: "V#000003" })).toBeUndefined();
      await t.transact([{ delete: { PK: pk, SK: "V#000002" } }]);
      expect(await t.get({ PK: pk, SK: "V#000002" })).toBeUndefined();
    });

    it("rejects an item over 400 KB and stores nothing", async () => {
      const t = getTable();
      const key = { PK: `PAGE#${uid()}`, SK: "LATEST" };
      await expect(t.put({ ...key, big: "x".repeat(401 * 1024) })).rejects.toThrow(/size/i);
      expect(await t.get(key)).toBeUndefined();
    });
  });
}
