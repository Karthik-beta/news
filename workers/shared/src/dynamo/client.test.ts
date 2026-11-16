// The SigV4 client against a fetch double: request shape, signing, endpoint guard and error mapping.
// Its behaviour against a real DynamoDB (Local) is covered by the api integration suite.
import { describe, expect, it } from "vitest";
import { DynamoTable, dynamoTableFromEnv, resolveEndpoint } from "./client";
import { eq } from "./conditions";
import { ConditionFailedError, DynamoError, OutcomeUnknownError, SubrequestLimitError, ThrottledError } from "./table";

const config = {
  table: "news",
  region: "us-east-1",
  accessKeyId: "AKIDEXAMPLE",
  secretAccessKey: "test-secret-not-real",
};

function recording(respond: (body: Record<string, unknown>, target: string) => Response) {
  const requests: Request[] = [];
  const fetch = async (r: Request) => {
    requests.push(r.clone());
    return respond(JSON.parse(await r.text()), r.headers.get("x-amz-target") ?? "");
  };
  return { requests, fetch };
}

const error = (type: string, status = 400, extra: object = {}) =>
  new Response(JSON.stringify({ __type: `com.amazonaws.dynamodb.v20120810#${type}`, message: "nope", ...extra }), {
    status,
  });

describe("resolveEndpoint", () => {
  it("defaults to the region's HTTPS endpoint and accepts it explicitly", () => {
    expect(resolveEndpoint("eu-west-2")).toBe("https://dynamodb.eu-west-2.amazonaws.com");
    expect(resolveEndpoint("eu-west-2", "https://dynamodb.eu-west-2.amazonaws.com/")).toBe(
      "https://dynamodb.eu-west-2.amazonaws.com",
    );
  });

  it("accepts loopback for DynamoDB Local", () => {
    expect(resolveEndpoint("us-east-1", "http://127.0.0.1:8000")).toBe("http://127.0.0.1:8000");
  });

  it.each([
    "https://evil.example.com",
    "http://dynamodb.us-east-1.amazonaws.com",
    "https://dynamodb.us-east-1.amazonaws.com.evil.com",
    "https://dynamodb.eu-west-1.amazonaws.com",
    "file:///etc/passwd",
    "not a url",
  ])("refuses %s", (endpoint) => {
    expect(() => resolveEndpoint("us-east-1", endpoint)).toThrow();
  });

  it("refuses a malformed region", () => {
    expect(() => resolveEndpoint("us-east-1.evil.com")).toThrow(/region/);
  });
});

describe("DynamoTable", () => {
  it("signs every request with SigV4 and the DynamoDB target header", async () => {
    const { requests, fetch } = recording(() => Response.json({}));
    const t = new DynamoTable({ ...config, fetch });
    expect(await t.get({ PK: "PAGE#a", SK: "LATEST" })).toBeUndefined();
    const r = requests[0] as Request;
    expect(r.url).toBe("https://dynamodb.us-east-1.amazonaws.com/");
    expect(r.headers.get("x-amz-target")).toBe("DynamoDB_20120810.GetItem");
    expect(r.headers.get("authorization")).toMatch(
      /^AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE\/\d{8}\/us-east-1\/dynamodb\//,
    );
    expect(await r.text()).toContain('"ConsistentRead":true');
  });

  it("reads strongly by default and eventually when asked (half the read units, for public readers)", async () => {
    const { requests, fetch } = recording(() => Response.json({ Items: [], Responses: { news: [] } }));
    const eventual = new DynamoTable({ ...config, fetch, consistentRead: false });
    expect(eventual.consistentRead).toBe(false);
    await eventual.get({ PK: "a", SK: "b" });
    await eventual.batchGet([{ PK: "a", SK: "b" }]);
    await eventual.query({ pk: "a" });
    for (const r of requests) expect(await r.text()).toContain('"ConsistentRead":false');
    expect(new DynamoTable({ ...config, fetch }).consistentRead).toBe(true);
  });

  it("sends conditions as placeholders and maps a failed condition", async () => {
    let sent: Record<string, unknown> = {};
    const { fetch } = recording((body) => {
      sent = body;
      return error("ConditionalCheckFailedException");
    });
    const t = new DynamoTable({ ...config, fetch });
    await expect(t.put({ PK: "a", SK: "b", x: 1 }, eq("state", "done"))).rejects.toBeInstanceOf(ConditionFailedError);
    expect(sent.ConditionExpression).toBe("#state = :v0");
    expect(sent.ExpressionAttributeValues).toEqual({ ":v0": { S: "done" } });
  });

  it.each(["ProvisionedThroughputExceededException", "ThrottlingException", "RequestLimitExceeded"])(
    "maps %s to ThrottledError",
    async (type) => {
      const t = new DynamoTable({ ...config, fetch: recording(() => error(type)).fetch });
      await expect(t.get({ PK: "a", SK: "b" })).rejects.toBeInstanceOf(ThrottledError);
    },
  );

  it("maps a cancelled transaction by its reasons", async () => {
    const cancelled = (code: string) =>
      error("TransactionCanceledException", 400, { CancellationReasons: [{ Code: "None" }, { Code: code }] });
    const cond = new DynamoTable({ ...config, fetch: recording(() => cancelled("ConditionalCheckFailed")).fetch });
    await expect(cond.transact([{ put: { PK: "a", SK: "b" } }])).rejects.toBeInstanceOf(ConditionFailedError);
    const thr = new DynamoTable({ ...config, fetch: recording(() => cancelled("ThrottlingError")).fetch });
    await expect(thr.transact([{ put: { PK: "a", SK: "b" } }])).rejects.toBeInstanceOf(ThrottledError);
    // Another transaction was writing one of the same items: nothing was written, so it is safe to retry.
    const busy = new DynamoTable({ ...config, fetch: recording(() => cancelled("TransactionConflict")).fetch });
    await expect(busy.transact([{ put: { PK: "a", SK: "b" } }])).rejects.toBeInstanceOf(ThrottledError);
  });

  it("other errors keep their type and never include the secret", async () => {
    const t = new DynamoTable({ ...config, fetch: recording(() => error("ValidationException")).fetch });
    const err = await t.get({ PK: "a", SK: "b" }).catch((e: unknown) => e);
    expect(err).toBeInstanceOf(DynamoError);
    expect((err as DynamoError).type).toBe("ValidationException");
    expect(String(err)).not.toContain(config.secretAccessKey);
  });

  it("a write with no answer or a 5xx is OutcomeUnknownError; a read stays an ordinary failure", async () => {
    const dropped = async () => {
      throw new DOMException("timed out", "TimeoutError");
    };
    const lost = new DynamoTable({ ...config, fetch: dropped });
    await expect(lost.put({ PK: "a", SK: "b" })).rejects.toBeInstanceOf(OutcomeUnknownError);
    await expect(lost.transact([{ put: { PK: "a", SK: "b" } }])).rejects.toBeInstanceOf(OutcomeUnknownError);
    await expect(lost.delete({ PK: "a", SK: "b" })).rejects.toBeInstanceOf(OutcomeUnknownError);
    const readErr = await lost.get({ PK: "a", SK: "b" }).catch((e: unknown) => e);
    expect(readErr).not.toBeInstanceOf(OutcomeUnknownError);
    const failing = new DynamoTable({ ...config, fetch: recording(() => error("InternalServerError", 500)).fetch });
    await expect(failing.put({ PK: "a", SK: "b" })).rejects.toBeInstanceOf(OutcomeUnknownError);
    await expect(failing.get({ PK: "a", SK: "b" })).rejects.toBeInstanceOf(DynamoError);
  });

  it("retries unprocessed batch keys, then gives up as throttled", async () => {
    let calls = 0;
    const { fetch } = recording((body) => {
      calls++;
      const keys = (body.RequestItems as Record<string, { Keys: unknown[] }>).news?.Keys ?? [];
      return Response.json({ Responses: { news: [] }, UnprocessedKeys: { news: { Keys: keys } } });
    });
    const t = new DynamoTable({ ...config, fetch });
    await expect(t.batchGet([{ PK: "a", SK: "b" }])).rejects.toBeInstanceOf(ThrottledError);
    expect(calls).toBe(4);
  });

  it("batchWrite: 25 writes per call, unprocessed ones retried twice, then throttled", async () => {
    const sizes: number[] = [];
    const { fetch } = recording((body) => {
      const items = (body.RequestItems as Record<string, unknown[]>).news ?? [];
      sizes.push(items.length);
      return Response.json({});
    });
    const t = new DynamoTable({ ...config, fetch });
    await t.batchWrite([
      ...Array.from({ length: 30 }, (_, i) => ({ put: { PK: "HUB#x", SK: `k${i}` } })),
      { delete: { PK: "HUB#x", SK: "gone" } },
    ]);
    expect(sizes).toEqual([25, 6]);
    let calls = 0;
    const stuck = recording((body) => {
      calls++;
      return Response.json({ UnprocessedItems: { news: (body.RequestItems as Record<string, unknown[]>).news } });
    });
    await expect(
      new DynamoTable({ ...config, fetch: stuck.fetch }).batchWrite([{ put: { PK: "a", SK: "b" } }]),
    ).rejects.toBeInstanceOf(ThrottledError);
    expect(calls).toBe(3);
  });

  it("over the subrequest limit a write is a plain failure (never sent), not an unknown outcome", async () => {
    const limited = async () => {
      throw new Error("Too many subrequests.");
    };
    const t = new DynamoTable({ ...config, fetch: limited });
    await expect(t.put({ PK: "a", SK: "b" })).rejects.toBeInstanceOf(SubrequestLimitError);
    await expect(t.batchWrite([{ put: { PK: "a", SK: "b" } }])).rejects.toBeInstanceOf(SubrequestLimitError);
    await expect(t.get({ PK: "a", SK: "b" })).rejects.toBeInstanceOf(SubrequestLimitError);
  });

  it("builds from env and refuses missing settings without echoing values", () => {
    expect(() => dynamoTableFromEnv({})).toThrow(/table name/);
    expect(() => dynamoTableFromEnv({ NEWS_DYNAMODB_TABLE: "news", NEWS_DYNAMODB_REGION: "us-east-1" })).toThrow(
      /credentials/,
    );
  });
});
