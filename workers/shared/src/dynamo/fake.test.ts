import { describe, expect, it } from "vitest";
import { compile, eq, evaluate, le, notExists, or } from "./conditions";
import { tableContract } from "./contract";
import { FakeTable } from "./fake";
import { fromItem, toItem } from "./codec";
import { type Doc, itemSize, ThrottledError, toDoc } from "./table";

const shared = new FakeTable();
tableContract("FakeTable", () => shared);

describe("FakeTable extras", () => {
  it("records calls and tells reads from writes", async () => {
    const t = new FakeTable();
    await t.get({ PK: "a", SK: "b" });
    await t.put({ PK: "a", SK: "b" });
    await t.query({ pk: "a" });
    expect(t.calls.map((c) => c.op)).toEqual(["get", "put", "query"]);
    expect(t.writes.map((c) => c.op)).toEqual(["put"]);
  });

  it("seed stores without recording; dump returns copies", async () => {
    const t = new FakeTable();
    t.seed({ PK: "a", SK: "b", n: 1 });
    expect(t.calls).toEqual([]);
    const [item] = t.dump();
    if (item) item.n = 99;
    expect((await t.get({ PK: "a", SK: "b" }))?.n).toBe(1);
  });

  it("can simulate throttling", async () => {
    const t = new FakeTable();
    t.throttleNext = 1;
    await expect(t.get({ PK: "a", SK: "b" })).rejects.toBeInstanceOf(ThrottledError);
    await expect(t.get({ PK: "a", SK: "b" })).resolves.toBeUndefined();
  });

  it("refuses two operations on one item in a transaction", async () => {
    const t = new FakeTable();
    await expect(t.transact([{ put: { PK: "a", SK: "b" } }, { delete: { PK: "a", SK: "b" } }])).rejects.toThrow(
      /multiple operations/,
    );
  });
});

describe("conditions", () => {
  it("compile uses placeholders for every name and value", () => {
    const c = compile(or(notExists(), le("version", 3)));
    expect(c.ConditionExpression).toBe("(attribute_not_exists(#PK) OR #version <= :v0)");
    expect(c.ExpressionAttributeNames).toEqual({ "#PK": "PK", "#version": "version" });
    expect(c.ExpressionAttributeValues).toEqual({ ":v0": 3 });
  });

  it("refuses attribute names that could inject expression syntax", () => {
    expect(() => compile(eq("a) OR (b", 1))).toThrow(/invalid attribute name/);
  });

  it("evaluate: types must match, missing is false", () => {
    expect(evaluate(eq("v", 1), { PK: "a", SK: "b", v: "1" })).toBe(false);
    expect(evaluate(le("v", 1), { PK: "a", SK: "b", v: 1 })).toBe(true);
    expect(evaluate(le("v", 1), undefined)).toBe(false);
  });
});

describe("toDoc", () => {
  it("converts a typed record value by value, leaving out undefined fields", () => {
    expect(toDoc({ PK: "a", SK: "b", n: 1, list: [{ x: null }], gone: undefined })).toEqual({
      PK: "a",
      SK: "b",
      n: 1,
      list: [{ x: null }],
    });
  });

  it.each([
    ["a Date", { d: new Date(0) }],
    ["a function", { f: () => 1 }],
    ["NaN", { n: Number.NaN }],
    ["a nested Map", { m: [new Map()] }],
  ])("refuses %s", (_label, value) => {
    expect(() => toDoc(value)).toThrow(TypeError);
  });
});

describe("a field named __proto__", () => {
  it("survives the DynamoDB codec and toDoc as an own field", () => {
    const doc = JSON.parse('{"PK":"a","SK":"b","data":{"__proto__":{"x":1}}}') as Doc;
    const item = toItem(doc);
    expect(Object.hasOwn((item.data as { M: object }).M, "__proto__")).toBe(true);
    const back = fromItem(item);
    expect(JSON.stringify(back)).toBe(JSON.stringify(doc));
    expect(JSON.stringify(toDoc(doc))).toBe(JSON.stringify(doc));
  });
});

describe("itemSize", () => {
  it("counts names and UTF-8 bytes", () => {
    expect(itemSize({ PK: "ab", SK: "é" })).toBe(2 + 2 + 2 + 2);
    expect(itemSize({ PK: "a", SK: "b", big: "x".repeat(1000) })).toBeGreaterThan(1000);
  });
});
