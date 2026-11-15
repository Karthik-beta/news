// Unit tests for the fail-fast DynamoDB check. `fetch` is stubbed: no network, no Docker.
import { afterEach, describe, expect, it, vi } from "vitest";
import { INFRA_HINT, requireDynamoDb } from "./service-check";

afterEach(() => {
  vi.unstubAllGlobals();
});
type FetchInput = Parameters<typeof fetch>[0];
const stubFetch = (impl: (input: FetchInput, init?: RequestInit) => Promise<Response>) => {
  const fn = vi.fn(impl);
  vi.stubGlobal("fetch", fn);
  return fn;
};
// What DynamoDB Local 3.3.2 answers to an unsigned GET /
const DYNAMO_400 = JSON.stringify({
  __type: "com.amazonaws.dynamodb.v20120810#MissingAuthenticationToken",
  Message: "Request must contain either a valid (registered) AWS access key ID or X.509 certificate.",
});

describe("requireDynamoDb", () => {
  it("passes when DynamoDB Local answers with its usual unsigned-request error", async () => {
    const fetchFn = stubFetch(async () => new Response(DYNAMO_400, { status: 400 }));
    await expect(requireDynamoDb({})).resolves.toBeUndefined();
    expect(String(fetchFn.mock.calls[0]?.[0])).toBe("http://127.0.0.1:8000");
  });

  it("passes for the older 'healthy: dynamodb' answer", async () => {
    stubFetch(async () => new Response("healthy: dynamodb.us-east-1.amazonaws.com ", { status: 200 }));
    await expect(requireDynamoDb({})).resolves.toBeUndefined();
  });

  it("fails with ONE line that names the endpoint and says to run bun run infra:up when nothing is listening", async () => {
    stubFetch(async () => {
      throw new TypeError("fetch failed");
    });
    const err = await requireDynamoDb({}).catch((e: Error) => e);
    expect(err).toBeInstanceOf(Error);
    expect((err as Error).message).toBe(`DynamoDB Local is not reachable at http://127.0.0.1:8000. ${INFRA_HINT}`);
    expect((err as Error).message).not.toContain("\n");
    expect(INFRA_HINT).toContain("bun run infra:up");
  });

  it("treats a timeout like an unreachable service", async () => {
    stubFetch(async () => {
      throw new DOMException("The operation timed out", "TimeoutError");
    });
    await expect(requireDynamoDb({})).rejects.toThrow(/not reachable.*infra:up/);
  });

  it("does not mistake some other service on the port for DynamoDB Local", async () => {
    stubFetch(async () => new Response("<html>Welcome to nginx</html>", { status: 200 }));
    await expect(requireDynamoDb({})).rejects.toThrow(/not DynamoDB Local.*infra:up/);
  });

  it("uses NEWS_DYNAMODB_ENDPOINT when it is set", async () => {
    const fetchFn = stubFetch(async () => new Response(DYNAMO_400, { status: 400 }));
    await requireDynamoDb({ NEWS_DYNAMODB_ENDPOINT: "http://localhost:8123/" });
    expect(String(fetchFn.mock.calls[0]?.[0])).toBe("http://localhost:8123");
  });

  it("refuses a non-loopback endpoint without making any request", async () => {
    const fetchFn = stubFetch(async () => new Response(DYNAMO_400, { status: 400 }));
    await expect(
      requireDynamoDb({ NEWS_DYNAMODB_ENDPOINT: "https://dynamodb.us-east-1.amazonaws.com" }),
    ).rejects.toThrow(/Refusing/);
    expect(fetchFn).not.toHaveBeenCalled();
  });

  it("gives up quickly: the request carries a timeout signal", async () => {
    const fetchFn = stubFetch(async () => new Response(DYNAMO_400, { status: 400 }));
    await requireDynamoDb({});
    expect(fetchFn.mock.calls[0]?.[1]?.signal).toBeInstanceOf(AbortSignal);
  });
});
