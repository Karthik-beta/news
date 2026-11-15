// Unit tests for the DynamoDB test client. No Docker, no network: `fetch` is replaced with recorded-style fixtures.
import { describe, expect, it, vi } from "vitest";
import {
  assertLocalEndpoint,
  DEFAULT_DYNAMO_CONFIG,
  DynamoDbLocal,
  DynamoError,
  dynamoBindings,
  dynamoConfigFromEnv,
  type FetchLike,
  fromItem,
  type Plain,
  toItem,
  uniqueTableName,
  withTempTable,
} from "./dynamodb";

type FetchInput = Parameters<typeof fetch>[0];

// Shapes copied from real DynamoDB Local 3.3.2 responses.
const ERR_CONDITIONAL = {
  __type: "com.amazonaws.dynamodb.v20120810#ConditionalCheckFailedException",
  message: "The conditional request failed",
};
const ERR_MISSING_AUTH = {
  __type: "com.amazonaws.dynamodb.v20120810#MissingAuthenticationToken",
  Message: "Request must contain either a valid (registered) AWS access key ID or X.509 certificate.",
};
const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { "content-type": "application/x-amz-json-1.0" } });
const fakeFetch = (...responses: Response[]) => {
  const requests: Request[] = [];
  const queue = [...responses];
  const impl = vi.fn(async (input: FetchInput) => {
    requests.push(input as Request);
    const next = queue.shift();
    if (!next) throw new Error("fake fetch: no response queued");
    return next;
  });
  return { impl: impl as unknown as FetchLike, requests };
};

describe("assertLocalEndpoint", () => {
  it.each([
    "http://127.0.0.1:8000",
    "http://localhost:8000",
    "http://[::1]:8000",
    "https://localhost",
    "http://127.0.0.1",
  ])("accepts %s", (endpoint) => {
    expect(assertLocalEndpoint(endpoint).href).toBeTruthy();
  });

  it.each([
    "https://dynamodb.us-east-1.amazonaws.com",
    "http://dynamodb.us-east-1.amazonaws.com:8000",
    "http://127.0.0.1.evil.com:8000",
    "http://localhost.evil.com",
    "http://127.0.0.1@evil.com",
    "http://0.0.0.0:8000",
    "http://192.168.1.10:8000",
    "http://host.docker.internal:8000",
    "ftp://127.0.0.1",
    "127.0.0.1:8000",
    "",
    "not a url",
  ])("rejects %j", (endpoint) => {
    expect(() => assertLocalEndpoint(endpoint)).toThrow();
  });
});

describe("dynamoConfigFromEnv", () => {
  it("falls back to the local defaults when nothing is set", () => {
    expect(dynamoConfigFromEnv({})).toEqual(DEFAULT_DYNAMO_CONFIG);
  });

  it("uses the NEWS_DYNAMODB_* values, trimmed, without a trailing slash", () => {
    const config = dynamoConfigFromEnv({
      NEWS_DYNAMODB_ENDPOINT: " http://localhost:8123/ ",
      NEWS_DYNAMODB_REGION: "eu-west-2",
      NEWS_DYNAMODB_ACCESS_KEY_ID: "AKIDOTHER",
      NEWS_DYNAMODB_SECRET_ACCESS_KEY: "other",
    });
    expect(config).toEqual({
      endpoint: "http://localhost:8123",
      region: "eu-west-2",
      accessKeyId: "AKIDOTHER",
      secretAccessKey: "other",
    });
  });

  it("treats empty, blank and non-string values as unset", () => {
    expect(
      dynamoConfigFromEnv({ NEWS_DYNAMODB_ENDPOINT: "", NEWS_DYNAMODB_REGION: "   ", NEWS_DYNAMODB_ACCESS_KEY_ID: 5 }),
    ).toEqual(DEFAULT_DYNAMO_CONFIG);
  });

  it("never picks up real AWS credentials or regions from the ordinary AWS_* variables", () => {
    const config = dynamoConfigFromEnv({
      AWS_ACCESS_KEY_ID: "AKIAREALKEY",
      AWS_SECRET_ACCESS_KEY: "real-secret",
      AWS_REGION: "ap-southeast-2",
    });
    expect(config).toEqual(DEFAULT_DYNAMO_CONFIG);
  });

  it("refuses a non-loopback endpoint", () => {
    expect(() => dynamoConfigFromEnv({ NEWS_DYNAMODB_ENDPOINT: "https://dynamodb.us-east-1.amazonaws.com" })).toThrow(
      /Refusing/,
    );
  });

  it("dynamoBindings carries all four settings under the same names", () => {
    const bindings = dynamoBindings(DEFAULT_DYNAMO_CONFIG);
    expect(Object.keys(bindings).sort()).toEqual([
      "NEWS_DYNAMODB_ACCESS_KEY_ID",
      "NEWS_DYNAMODB_ENDPOINT",
      "NEWS_DYNAMODB_REGION",
      "NEWS_DYNAMODB_SECRET_ACCESS_KEY",
    ]);
    expect(dynamoConfigFromEnv(bindings)).toEqual(DEFAULT_DYNAMO_CONFIG);
  });
});

describe("attribute conversion", () => {
  const sample: { [key: string]: Plain } = {
    s: "text",
    empty: "",
    unicode: "café \u{1F4C8}",
    int: 3,
    negative: -7,
    float: 1.5,
    zero: 0,
    yes: true,
    no: false,
    nothing: null,
    list: [1, "two", [3], { four: 4 }],
    emptyList: [],
    map: { nested: { deep: ["x"] } },
    emptyMap: {},
  };

  it("round-trips every supported type, including empties and nesting", () => {
    expect(fromItem(toItem(sample))).toEqual(sample);
  });

  it("encodes numbers as strings of type N, as DynamoDB expects", () => {
    expect(toItem({ n: 12, f: 0.5 })).toEqual({ n: { N: "12" }, f: { N: "0.5" } });
  });

  it.each([Number.NaN, Number.POSITIVE_INFINITY, Number.NEGATIVE_INFINITY])("rejects %s", (value) => {
    expect(() => toItem({ value })).toThrow(/cannot store/);
  });
});

describe("DynamoDbLocal requests", () => {
  it("signs with SigV4 for the dynamodb service and the configured region and key", async () => {
    const { impl, requests } = fakeFetch(json({}));
    await new DynamoDbLocal(DEFAULT_DYNAMO_CONFIG, impl).call("ListTables", {});
    const [req] = requests;
    expect(req?.method).toBe("POST");
    expect(req?.url).toBe("http://127.0.0.1:8000/");
    expect(req?.headers.get("x-amz-target")).toBe("DynamoDB_20120810.ListTables");
    expect(req?.headers.get("content-type")).toBe("application/x-amz-json-1.0");
    expect(req?.headers.get("x-amz-date")).toMatch(/^\d{8}T\d{6}Z$/);
    expect(req?.headers.get("authorization")).toMatch(
      /^AWS4-HMAC-SHA256 Credential=AKIDLOCAL\/\d{8}\/us-east-1\/dynamodb\/aws4_request, SignedHeaders=[^,]*x-amz-target[^,]*, Signature=[0-9a-f]{64}$/,
    );
  });

  it("uses the region and key it was configured with", async () => {
    const { impl, requests } = fakeFetch(json({}));
    await new DynamoDbLocal({ ...DEFAULT_DYNAMO_CONFIG, region: "eu-west-2", accessKeyId: "AKIDOTHER" }, impl).call(
      "ListTables",
      {},
    );
    expect(requests[0]?.headers.get("authorization")).toContain("Credential=AKIDOTHER/");
    expect(requests[0]?.headers.get("authorization")).toContain("/eu-west-2/dynamodb/aws4_request");
  });

  it("signs the body: a different body gives a different signature", async () => {
    const { impl, requests } = fakeFetch(json({}), json({}));
    const client = new DynamoDbLocal(DEFAULT_DYNAMO_CONFIG, impl);
    await client.call("GetItem", { TableName: "a" });
    await client.call("GetItem", { TableName: "b" });
    const sig = (r?: Request) => r?.headers.get("authorization")?.split("Signature=")[1];
    expect(sig(requests[0])).toBeTruthy();
    expect(sig(requests[0])).not.toBe(sig(requests[1]));
  });

  it("sends no Authorization header when asked for an unsigned request", async () => {
    const { impl, requests } = fakeFetch(json(ERR_MISSING_AUTH, 400));
    await expect(
      new DynamoDbLocal(DEFAULT_DYNAMO_CONFIG, impl).call("ListTables", {}, { sign: false }),
    ).rejects.toThrow();
    expect(requests[0]?.headers.get("authorization")).toBeNull();
  });

  it("sends the body as JSON", async () => {
    const { impl, requests } = fakeFetch(json({}));
    await new DynamoDbLocal(DEFAULT_DYNAMO_CONFIG, impl).call("DeleteTable", { TableName: "t" });
    expect(await requests[0]?.json()).toEqual({ TableName: "t" });
  });

  it("refuses to be built for a non-loopback endpoint, before any request", () => {
    const { impl, requests } = fakeFetch();
    expect(
      () => new DynamoDbLocal({ ...DEFAULT_DYNAMO_CONFIG, endpoint: "https://dynamodb.eu-west-1.amazonaws.com" }, impl),
    ).toThrow(/Refusing/);
    expect(requests).toHaveLength(0);
  });
});

describe("DynamoDbLocal responses", () => {
  const client = (...responses: Response[]) => new DynamoDbLocal(DEFAULT_DYNAMO_CONFIG, fakeFetch(...responses).impl);

  it("maps the long __type to the short error name and keeps status and message", async () => {
    const err = await client(json(ERR_CONDITIONAL, 400))
      .call("PutItem", {})
      .catch((e) => e);
    expect(err).toBeInstanceOf(DynamoError);
    expect(err).toMatchObject({
      type: "ConditionalCheckFailedException",
      status: 400,
      message: expect.stringContaining("conditional request failed"),
    });
  });

  it("reads the capitalised Message field some errors use", async () => {
    const err = await client(json(ERR_MISSING_AUTH, 400))
      .call("ListTables", {})
      .catch((e) => e);
    expect(err.type).toBe("MissingAuthenticationToken");
    expect(err.message).toContain("valid (registered) AWS access key ID");
  });

  it("copes with a non-JSON error body", async () => {
    const err = await client(new Response("<html>Bad gateway</html>", { status: 502 }))
      .call("ListTables", {})
      .catch((e) => e);
    expect(err).toMatchObject({ type: "UnknownError", status: 502 });
    expect(err.message).toContain("Bad gateway");
  });

  it("copes with an empty error body", async () => {
    const err = await client(new Response("", { status: 500, statusText: "Internal Server Error" }))
      .call("ListTables", {})
      .catch((e) => e);
    expect(err).toMatchObject({ type: "UnknownError", status: 500 });
  });

  it("returns an empty object for an empty success body", async () => {
    expect(await client(new Response("", { status: 200 })).call("DeleteItem", {})).toEqual({});
  });

  it("getItem returns the item, or undefined when the response has none", async () => {
    const item = { PK: { S: "a" }, SK: { S: "b" } };
    expect(await client(json({ Item: item })).getItem("t", { PK: "a", SK: "b" })).toEqual(item);
    expect(await client(json({})).getItem("t", { PK: "a", SK: "b" })).toBeUndefined();
  });

  it("deleteTableIfExists swallows only ResourceNotFoundException", async () => {
    const notFound = {
      __type: "com.amazonaws.dynamodb.v20120810#ResourceNotFoundException",
      message: "Cannot do operations on a non-existent table",
    };
    await expect(client(json(notFound, 400)).deleteTableIfExists("t")).resolves.toBeUndefined();
    const denied = { __type: "com.amazonaws.dynamodb.v20120810#AccessDeniedException", message: "no" };
    await expect(client(json(denied, 400)).deleteTableIfExists("t")).rejects.toMatchObject({
      type: "AccessDeniedException",
    });
  });

  it("query asks for a consistent read by default and lets the caller override it", async () => {
    const a = fakeFetch(json({ Items: [], Count: 0 }), json({ Items: [], Count: 0 }));
    const c = new DynamoDbLocal(DEFAULT_DYNAMO_CONFIG, a.impl);
    await c.query("t", { Limit: 1 });
    await c.query("t", { ConsistentRead: false });
    expect(await a.requests[0]?.json()).toMatchObject({ TableName: "t", ConsistentRead: true, Limit: 1 });
    expect(await a.requests[1]?.json()).toMatchObject({ ConsistentRead: false });
  });

  it("createSingleTable declares string PK (hash) and SK (range) keys", async () => {
    const f = fakeFetch(json({}));
    await new DynamoDbLocal(DEFAULT_DYNAMO_CONFIG, f.impl).createSingleTable("t");
    expect(await f.requests[0]?.json()).toMatchObject({
      TableName: "t",
      AttributeDefinitions: [
        { AttributeName: "PK", AttributeType: "S" },
        { AttributeName: "SK", AttributeType: "S" },
      ],
      KeySchema: [
        { AttributeName: "PK", KeyType: "HASH" },
        { AttributeName: "SK", KeyType: "RANGE" },
      ],
    });
  });
});

describe("uniqueTableName", () => {
  it("makes valid, distinct DynamoDB table names", () => {
    const names = Array.from({ length: 500 }, () => uniqueTableName());
    expect(new Set(names).size).toBe(500);
    for (const name of names) expect(name).toMatch(/^news-test-[a-z0-9]+-[0-9a-f]{8}$/);
  });

  it("keeps a custom prefix and stays inside the 255 character limit", () => {
    expect(uniqueTableName("news-test-missing")).toMatch(/^news-test-missing-/);
    expect(uniqueTableName().length).toBeLessThanOrEqual(255);
  });
});

describe("withTempTable", () => {
  const ok = () => json({});
  const notFound = () =>
    json({ __type: "com.amazonaws.dynamodb.v20120810#ResourceNotFoundException", message: "gone" }, 400);
  const targets = (requests: Request[]) => requests.map((r) => r.headers.get("x-amz-target")?.split(".")[1]);

  it("creates the table, passes its name to fn, deletes it, and returns fn's result", async () => {
    const f = fakeFetch(ok(), ok());
    let seen = "";
    const result = await withTempTable(new DynamoDbLocal(DEFAULT_DYNAMO_CONFIG, f.impl), async (table) => {
      seen = table;
      return 42;
    });
    expect(result).toBe(42);
    expect(targets(f.requests)).toEqual(["CreateTable", "DeleteTable"]);
    expect(await f.requests[0]?.json()).toMatchObject({ TableName: seen });
    expect(await f.requests[1]?.json()).toEqual({ TableName: seen });
  });

  it("still deletes the table when fn throws, and rethrows fn's error", async () => {
    const f = fakeFetch(ok(), ok());
    await expect(
      withTempTable(new DynamoDbLocal(DEFAULT_DYNAMO_CONFIG, f.impl), async () => {
        throw new Error("test body failed");
      }),
    ).rejects.toThrow("test body failed");
    expect(targets(f.requests)).toEqual(["CreateTable", "DeleteTable"]);
  });

  it("a failing cleanup does not hide the error from fn", async () => {
    const f = fakeFetch(ok(), json({ __type: "x#InternalServerError", message: "cleanup broke" }, 500));
    await expect(
      withTempTable(new DynamoDbLocal(DEFAULT_DYNAMO_CONFIG, f.impl), async () => {
        throw new Error("test body failed");
      }),
    ).rejects.toThrow("test body failed");
  });

  it("surfaces a cleanup failure when fn itself succeeded", async () => {
    const f = fakeFetch(ok(), json({ __type: "x#InternalServerError", message: "cleanup broke" }, 500));
    await expect(withTempTable(new DynamoDbLocal(DEFAULT_DYNAMO_CONFIG, f.impl), async () => "fine")).rejects.toThrow(
      /cleanup broke/,
    );
  });

  it("does not run fn, and does not try to delete, when the table cannot be created", async () => {
    const f = fakeFetch(json({ __type: "x#ResourceInUseException", message: "exists" }, 400));
    const fn = vi.fn(async () => "never");
    await expect(withTempTable(new DynamoDbLocal(DEFAULT_DYNAMO_CONFIG, f.impl), fn)).rejects.toMatchObject({
      type: "ResourceInUseException",
    });
    expect(fn).not.toHaveBeenCalled();
    expect(targets(f.requests)).toEqual(["CreateTable"]);
  });

  it("is happy when fn already deleted the table itself", async () => {
    const f = fakeFetch(ok(), notFound());
    await expect(withTempTable(new DynamoDbLocal(DEFAULT_DYNAMO_CONFIG, f.impl), async () => "done")).resolves.toBe(
      "done",
    );
  });
});
