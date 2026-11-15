// The MCP Worker is read-only and reads page records and hub listings from DynamoDB. This proves the Worker gets the
// local settings and can sign, read and query against DynamoDB Local. The full behaviour suite lives in the api tests.
// Needs `bun run infra:up`.
import { env } from "cloudflare:workers";
import {
  DynamoDbLocal,
  DynamoError,
  dynamoConfigFromEnv,
  fromItem,
  toItem,
  withTempTable,
} from "@news/worker-test-support/dynamodb";
import { describe, expect, it } from "vitest";

const client = new DynamoDbLocal(dynamoConfigFromEnv(env as unknown as Record<string, unknown>));

describe("DynamoDB Local from the mcp Worker", () => {
  it("has the local settings as bindings", () => {
    expect(env.NEWS_DYNAMODB_ENDPOINT).toMatch(/^http:\/\/(127\.0\.0\.1|localhost):\d+$/);
    expect(env.NEWS_DYNAMODB_REGION).toBe("us-east-1");
  });

  it("reads a seeded page record and lists a hub, and misses are undefined / empty", async () => {
    await withTempTable(client, async (table) => {
      await client.putItem(table, toItem({ PK: "PAGE#p1", SK: "LATEST", title: "First page", version: 1 }));
      await client.putItem(table, toItem({ PK: "HUB#economy", SK: "2026-11-02#p2" }));
      await client.putItem(table, toItem({ PK: "HUB#economy", SK: "2026-11-01#p1" }));

      const page = await client.getItem(table, { PK: "PAGE#p1", SK: "LATEST" });
      expect(page && fromItem(page)).toEqual({ PK: "PAGE#p1", SK: "LATEST", title: "First page", version: 1 });
      expect(await client.getItem(table, { PK: "PAGE#nope", SK: "LATEST" })).toBeUndefined();

      const hub = await client.query(table, {
        KeyConditionExpression: "PK = :pk",
        ExpressionAttributeValues: toItem({ ":pk": "HUB#economy" }),
        ScanIndexForward: false,
      });
      expect(hub.Items.map((i) => fromItem(i).SK)).toEqual(["2026-11-02#p2", "2026-11-01#p1"]);
      expect(
        (
          await client.query(table, {
            KeyConditionExpression: "PK = :pk",
            ExpressionAttributeValues: toItem({ ":pk": "HUB#none" }),
          })
        ).Count,
      ).toBe(0);
    });
  });

  it("reports a missing table as ResourceNotFoundException (what the Worker sees before the table is created)", async () => {
    await expect(client.getItem("news-test-no-such-table", { PK: "a", SK: "b" })).rejects.toMatchObject({
      type: "ResourceNotFoundException",
    });
    await expect(client.getItem("news-test-no-such-table", { PK: "a", SK: "b" })).rejects.toBeInstanceOf(DynamoError);
  });
});
