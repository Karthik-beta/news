// DynamoDB Local over signed HTTP, called from inside workerd, using the single-table key shapes from the
// Technical Design (PAGE#<id> / V#<version> and LATEST, HUB#<category> / <date>#<page id>). Needs `bun run infra:up`.
import { env } from "cloudflare:workers";
import {
  DynamoDbLocal,
  DynamoError,
  dynamoConfigFromEnv,
  fromItem,
  type Plain,
  type QueryResult,
  toItem,
  uniqueTableName,
  withTempTable,
} from "@news/worker-test-support/dynamodb";
import { afterAll, beforeAll, describe, expect, it } from "vitest";

const client = new DynamoDbLocal(dynamoConfigFromEnv(env as unknown as Record<string, unknown>));
const table = uniqueTableName();
const failure = async (p: Promise<unknown>): Promise<DynamoError> => {
  try {
    await p;
  } catch (e) {
    expect(e).toBeInstanceOf(DynamoError);
    return e as DynamoError;
  }
  throw new Error("expected the DynamoDB call to fail, but it succeeded");
};
const ids = () => crypto.randomUUID().slice(0, 8);

beforeAll(async () => {
  await client.createSingleTable(table);
});
afterAll(async () => {
  await client.deleteTableIfExists(table);
  const { TableNames } = await client.call<{ TableNames: string[] }>("ListTables", {});
  expect(TableNames).not.toContain(table);
});

describe("connection and signing", () => {
  it("the Worker got the local settings as bindings, and they point at loopback", () => {
    expect(env.NEWS_DYNAMODB_ENDPOINT).toMatch(/^http:\/\/(127\.0\.0\.1|localhost):\d+$/);
    expect(env.NEWS_DYNAMODB_ACCESS_KEY_ID).toBe("AKIDLOCAL");
  });

  it("a signed request works and lists the throwaway table", async () => {
    const { TableNames } = await client.call<{ TableNames: string[] }>("ListTables", {});
    expect(TableNames).toContain(table);
  });

  it("an unsigned request is rejected", async () => {
    const err = await failure(client.call("ListTables", {}, { sign: false }));
    expect(err.type).toBe("MissingAuthenticationToken");
    expect(err.status).toBe(400);
  });

  it("refuses to be pointed anywhere except loopback", () => {
    expect(() => new DynamoDbLocal({ ...client.config, endpoint: "https://dynamodb.us-east-1.amazonaws.com" })).toThrow(
      /Refusing/,
    );
  });
});

describe("page records", () => {
  it("puts and gets an item back unchanged, including text that needs escaping", async () => {
    const id = `PAGE#${ids()}`;
    const record = {
      PK: id,
      SK: "LATEST",
      title: 'Nvidia Q3: "record" revenue — up 94%',
      version: 3,
      published: true,
      supersedes: null,
      categories: ["companies", "ai"],
      oui: 'root = Page(title="x")\n\tline two with \u{1F4C8} and café',
      meta: { region: "US", sources: ["sec-edgar"], count: 2 },
    };
    await client.putItem(table, toItem(record));
    const item = await client.getItem(table, { PK: id, SK: "LATEST" });
    expect(item && fromItem(item)).toEqual(record);
  });

  it("returns undefined for a missing item, also when only a different sort key exists", async () => {
    const id = `PAGE#${ids()}`;
    expect(await client.getItem(table, { PK: id, SK: "LATEST" })).toBeUndefined();
    await client.putItem(table, toItem({ PK: id, SK: "V#1" }));
    expect(await client.getItem(table, { PK: id, SK: "LATEST" })).toBeUndefined();
  });

  it("a second put replaces the whole item (no attribute from the first survives)", async () => {
    const key = { PK: `PAGE#${ids()}`, SK: "LATEST" };
    await client.putItem(table, toItem({ ...key, title: "old", legacy: "x" }));
    await client.putItem(table, toItem({ ...key, title: "new" }));
    expect(fromItem((await client.getItem(table, key)) ?? {})).toEqual({ ...key, title: "new" });
  });

  it("create-only puts: the second create fails and leaves the first item intact", async () => {
    const key = { PK: `PAGE#${ids()}`, SK: "V#1" };
    const create = (title: string) =>
      client.putItem(table, toItem({ ...key, title }), { ConditionExpression: "attribute_not_exists(PK)" });
    await create("first");
    const err = await failure(create("second"));
    expect(err.type).toBe("ConditionalCheckFailedException");
    expect(fromItem((await client.getItem(table, key)) ?? {}).title).toBe("first");
  });

  it("optimistic update: only the writer that saw the current version wins", async () => {
    const key = { PK: `PAGE#${ids()}`, SK: "LATEST" };
    await client.putItem(table, toItem({ ...key, version: 1 }));
    const bump = (expected: number, next: number) =>
      client.putItem(table, toItem({ ...key, version: next }), {
        ConditionExpression: "version = :expected",
        ExpressionAttributeValues: toItem({ ":expected": expected }),
      });
    await bump(1, 2);
    expect((await failure(bump(1, 3))).type).toBe("ConditionalCheckFailedException");
    expect(fromItem((await client.getItem(table, key)) ?? {}).version).toBe(2);
  });

  it("delete removes the item, and deleting it again is not an error", async () => {
    const key = { PK: `PAGE#${ids()}`, SK: "LATEST" };
    await client.putItem(table, toItem(key));
    await client.deleteItem(table, key);
    expect(await client.getItem(table, key)).toBeUndefined();
    await expect(client.deleteItem(table, key)).resolves.toBeDefined();
  });
});

describe("queries", () => {
  const keysOf = (r: QueryResult) => r.Items.map((i) => fromItem(i).SK);

  it("versions of a page come back in sort-key order, newest first when asked", async () => {
    const PK = `PAGE#${ids()}`;
    for (const SK of ["V#0001", "V#0002", "V#0003", "LATEST"]) await client.putItem(table, toItem({ PK, SK }));
    const versions = {
      KeyConditionExpression: "PK = :pk AND begins_with(SK, :v)",
      ExpressionAttributeValues: toItem({ ":pk": PK, ":v": "V#" }),
    };
    expect(keysOf(await client.query(table, versions))).toEqual(["V#0001", "V#0002", "V#0003"]);
    expect(keysOf(await client.query(table, { ...versions, ScanIndexForward: false, Limit: 1 }))).toEqual(["V#0003"]);
  });

  it("sort keys are strings: unpadded version numbers sort V#1, V#10, V#2", async () => {
    // Pins DynamoDB's ordering so nobody relies on `V#<n>` sorting numerically. Newest-first queries over
    // unpadded versions break at version 10, so use the LATEST item or zero-pad the version.
    const PK = `PAGE#${ids()}`;
    for (const SK of ["V#1", "V#2", "V#10"]) await client.putItem(table, toItem({ PK, SK }));
    const res = await client.query(table, {
      KeyConditionExpression: "PK = :pk",
      ExpressionAttributeValues: toItem({ ":pk": PK }),
    });
    expect(keysOf(res)).toEqual(["V#1", "V#10", "V#2"]);
  });

  it("a query only returns items of its own partition", async () => {
    const [a, b] = [`HUB#${ids()}`, `HUB#${ids()}`];
    await client.putItem(table, toItem({ PK: a, SK: "2026-11-01#p1" }));
    await client.putItem(table, toItem({ PK: b, SK: "2026-11-01#p2" }));
    const res = await client.query(table, {
      KeyConditionExpression: "PK = :pk",
      ExpressionAttributeValues: toItem({ ":pk": a }),
    });
    expect(keysOf(res)).toEqual(["2026-11-01#p1"]);
    expect(res.Count).toBe(1);
  });

  it("an unknown partition returns an empty result, not an error", async () => {
    const res = await client.query(table, {
      KeyConditionExpression: "PK = :pk",
      ExpressionAttributeValues: toItem({ ":pk": `HUB#${ids()}` }),
    });
    expect(res).toMatchObject({ Items: [], Count: 0 });
    expect(res.LastEvaluatedKey).toBeUndefined();
  });

  it("hub listing: a date range, newest first, paged to the end without gaps or repeats", async () => {
    const PK = `HUB#economy-${ids()}`;
    const days = ["2026-11-01", "2026-11-02", "2026-11-03", "2026-11-04", "2026-11-05"];
    for (const day of days) await client.putItem(table, toItem({ PK, SK: `${day}#page-${day}` }));

    const range = {
      KeyConditionExpression: "PK = :pk AND SK BETWEEN :from AND :to",
      ExpressionAttributeValues: toItem({ ":pk": PK, ":from": "2026-11-02", ":to": "2026-11-04#￿" }),
      ScanIndexForward: false,
      Limit: 2,
    };
    const seen: Plain[] = [];
    let start: Record<string, unknown> | undefined;
    let pages = 0;
    do {
      const res = await client.query(table, { ...range, ExclusiveStartKey: start });
      seen.push(...keysOf(res));
      start = res.LastEvaluatedKey;
      pages++;
    } while (start && pages < 10);
    expect(seen).toEqual(["2026-11-04#page-2026-11-04", "2026-11-03#page-2026-11-03", "2026-11-02#page-2026-11-02"]);
    expect(pages).toBe(2);
  });
});

describe("validation and error paths", () => {
  it("rejects an item with no sort key", async () => {
    const err = await failure(client.putItem(table, toItem({ PK: `PAGE#${ids()}` })));
    expect(err.type).toBe("ValidationException");
  });

  it("rejects a key attribute of the wrong type", async () => {
    const err = await failure(client.putItem(table, toItem({ PK: `PAGE#${ids()}`, SK: 5 })));
    expect(err.type).toBe("ValidationException");
  });

  it("rejects an item over the 400 KB limit and stores nothing", async () => {
    const key = { PK: `PAGE#${ids()}`, SK: "LATEST" };
    const err = await failure(client.putItem(table, toItem({ ...key, oui: "x".repeat(401 * 1024) })));
    expect(err.type).toBe("ValidationException");
    expect(err.message).toMatch(/400\s?KB|item size/i);
    expect(await client.getItem(table, key)).toBeUndefined();
  });

  it("accepts an item just under the limit", async () => {
    const key = { PK: `PAGE#${ids()}`, SK: "LATEST" };
    await client.putItem(table, toItem({ ...key, oui: "x".repeat(390 * 1024) }));
    expect(await client.getItem(table, key)).toBeDefined();
  });

  it("rejects a malformed key condition expression", async () => {
    const err = await failure(
      client.query(table, { KeyConditionExpression: "PK ===", ExpressionAttributeValues: toItem({ ":pk": "x" }) }),
    );
    expect(err.type).toBe("ValidationException");
  });

  it("rejects an unknown operation", async () => {
    const err = await failure(client.call("NoSuchOperation", {}));
    expect(err.status).toBe(400);
  });

  it("fails with ResourceNotFoundException for a table that does not exist", async () => {
    const err = await failure(client.getItem(uniqueTableName("news-test-missing"), { PK: "a", SK: "b" }));
    expect(err.type).toBe("ResourceNotFoundException");
  });
});

describe("table lifecycle", () => {
  it("creating a table that exists fails with ResourceInUseException", async () => {
    expect((await failure(client.createSingleTable(table))).type).toBe("ResourceInUseException");
  });

  it("withTempTable gives a usable table and deletes it afterwards", async () => {
    let name = "";
    await withTempTable(client, async (t) => {
      name = t;
      await client.putItem(t, toItem({ PK: "a", SK: "b" }));
      expect(await client.getItem(t, { PK: "a", SK: "b" })).toBeDefined();
    });
    expect((await failure(client.getItem(name, { PK: "a", SK: "b" }))).type).toBe("ResourceNotFoundException");
  });

  it("withTempTable still deletes the table when the test body throws", async () => {
    let name = "";
    await expect(
      withTempTable(client, async (t) => {
        name = t;
        throw new Error("boom");
      }),
    ).rejects.toThrow("boom");
    const { TableNames } = await client.call<{ TableNames: string[] }>("ListTables", {});
    expect(TableNames).not.toContain(name);
  });

  it("deleting a table twice: the first succeeds, deleteTableIfExists tolerates the second, deleteTable does not", async () => {
    const name = uniqueTableName();
    await client.createSingleTable(name);
    await client.deleteTable(name);
    await expect(client.deleteTableIfExists(name)).resolves.toBeUndefined();
    expect((await failure(client.deleteTable(name))).type).toBe("ResourceNotFoundException");
  });

  it("table names are unique per call", () => {
    expect(new Set(Array.from({ length: 200 }, () => uniqueTableName())).size).toBe(200);
  });
});
