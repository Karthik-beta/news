// A small DynamoDB client for tests, usable both inside workerd and in Node.
//
// It talks to AWS's own DynamoDB Local over HTTP with SigV4-signed requests (aws4fetch), the same way the
// Workers will talk to the real service (Technical Design: "DynamoDB is called from Workers over HTTPS with
// signed requests"). It refuses any endpoint that is not loopback, so a test can never reach a real AWS account.
import { AwsClient } from "aws4fetch";

export interface DynamoConfig {
  endpoint: string;
  region: string;
  accessKeyId: string;
  secretAccessKey: string;
}

/** Dummy values for DynamoDB Local (`infra/docker-compose.yml`, `.env.example`). Not real credentials. */
export const DEFAULT_DYNAMO_CONFIG: DynamoConfig = {
  endpoint: "http://127.0.0.1:8000",
  region: "us-east-1",
  accessKeyId: "AKIDLOCAL",
  secretAccessKey: "local",
};

/** Names of the settings, shared by `.env`, the Node test process and the Worker bindings the harness injects. */
export const DYNAMO_ENV_KEYS = {
  endpoint: "NEWS_DYNAMODB_ENDPOINT",
  region: "NEWS_DYNAMODB_REGION",
  accessKeyId: "NEWS_DYNAMODB_ACCESS_KEY_ID",
  secretAccessKey: "NEWS_DYNAMODB_SECRET_ACCESS_KEY",
} as const;

const LOOPBACK_HOSTS = new Set(["127.0.0.1", "localhost", "[::1]"]);

/** Throws unless `endpoint` is an http(s) URL on a loopback host. Returns the parsed URL. */
export function assertLocalEndpoint(endpoint: string): URL {
  let url: URL;
  try {
    url = new URL(endpoint);
  } catch {
    throw new Error(`DynamoDB endpoint is not a valid URL: ${JSON.stringify(endpoint)}`);
  }
  if (!["http:", "https:"].includes(url.protocol) || !LOOPBACK_HOSTS.has(url.hostname)) {
    throw new Error(
      `Refusing DynamoDB endpoint ${endpoint}: tests only talk to DynamoDB Local on 127.0.0.1/localhost.`,
    );
  }
  return url;
}

/** Reads the settings from an env-like object (process.env in Node, the Worker `env` in workerd). Unset or empty values fall back to the defaults. */
export function dynamoConfigFromEnv(env: Record<string, unknown>): DynamoConfig {
  const pick = (key: string, fallback: string) => {
    const v = env[key];
    return typeof v === "string" && v.trim() !== "" ? v.trim() : fallback;
  };
  const config: DynamoConfig = {
    endpoint: pick(DYNAMO_ENV_KEYS.endpoint, DEFAULT_DYNAMO_CONFIG.endpoint).replace(/\/+$/, ""),
    region: pick(DYNAMO_ENV_KEYS.region, DEFAULT_DYNAMO_CONFIG.region),
    accessKeyId: pick(DYNAMO_ENV_KEYS.accessKeyId, DEFAULT_DYNAMO_CONFIG.accessKeyId),
    secretAccessKey: pick(DYNAMO_ENV_KEYS.secretAccessKey, DEFAULT_DYNAMO_CONFIG.secretAccessKey),
  };
  assertLocalEndpoint(config.endpoint);
  return config;
}

/** The same settings as plain Worker bindings, for the vitest harness to inject. */
export function dynamoBindings(config: DynamoConfig): Record<string, string> {
  return {
    [DYNAMO_ENV_KEYS.endpoint]: config.endpoint,
    [DYNAMO_ENV_KEYS.region]: config.region,
    [DYNAMO_ENV_KEYS.accessKeyId]: config.accessKeyId,
    [DYNAMO_ENV_KEYS.secretAccessKey]: config.secretAccessKey,
  };
}

// --- attribute values -------------------------------------------------------------------------------------------

export type AttributeValue =
  | { S: string }
  | { N: string }
  | { BOOL: boolean }
  | { NULL: true }
  | { L: AttributeValue[] }
  | { M: Record<string, AttributeValue> };
export type Item = Record<string, AttributeValue>;
export type Plain = string | number | boolean | null | Plain[] | { [key: string]: Plain };

export function toAttribute(value: Plain): AttributeValue {
  if (value === null) return { NULL: true };
  if (typeof value === "string") return { S: value };
  if (typeof value === "number") {
    if (!Number.isFinite(value)) throw new Error(`DynamoDB cannot store ${value}`);
    return { N: String(value) };
  }
  if (typeof value === "boolean") return { BOOL: value };
  if (Array.isArray(value)) return { L: value.map(toAttribute) };
  return { M: toItem(value) };
}

export function toItem(object: { [key: string]: Plain }): Item {
  return Object.fromEntries(Object.entries(object).map(([k, v]) => [k, toAttribute(v)]));
}

export function fromAttribute(attr: AttributeValue): Plain {
  if ("S" in attr) return attr.S;
  if ("N" in attr) return Number(attr.N);
  if ("BOOL" in attr) return attr.BOOL;
  if ("NULL" in attr) return null;
  if ("L" in attr) return attr.L.map(fromAttribute);
  return fromItem(attr.M);
}

export function fromItem(item: Item): { [key: string]: Plain } {
  return Object.fromEntries(Object.entries(item).map(([k, v]) => [k, fromAttribute(v)]));
}

// --- client -----------------------------------------------------------------------------------------------------

/** An error response from DynamoDB: `type` is the short name, e.g. `ConditionalCheckFailedException`. */
export class DynamoError extends Error {
  constructor(
    readonly type: string,
    message: string,
    readonly status: number,
  ) {
    super(`${type}: ${message}`);
    this.name = "DynamoError";
  }
}

export interface QueryResult {
  Items: Item[];
  Count: number;
  LastEvaluatedKey?: Item;
}

const TARGET_PREFIX = "DynamoDB_20120810.";

/** Just the call signature of fetch: runtimes add extras (Bun's `preconnect`) that test doubles need not have. */
export type FetchLike = (input: Parameters<typeof fetch>[0], init?: RequestInit) => Promise<Response>;

export class DynamoDbLocal {
  readonly config: DynamoConfig;
  readonly #aws: AwsClient;
  readonly #fetch: FetchLike;

  constructor(
    config: DynamoConfig = DEFAULT_DYNAMO_CONFIG,
    fetchImpl: FetchLike = (input, init) => fetch(input, init),
  ) {
    assertLocalEndpoint(config.endpoint);
    this.config = config;
    this.#fetch = fetchImpl;
    this.#aws = new AwsClient({
      accessKeyId: config.accessKeyId,
      secretAccessKey: config.secretAccessKey,
      region: config.region,
      service: "dynamodb",
      retries: 0,
    });
  }

  /** Calls one DynamoDB operation with a signed request. Throws DynamoError on a non-2xx response. */
  async call<T = Record<string, unknown>>(operation: string, body: object, opts: { sign?: boolean } = {}): Promise<T> {
    const init = {
      method: "POST",
      headers: { "content-type": "application/x-amz-json-1.0", "x-amz-target": TARGET_PREFIX + operation },
      body: JSON.stringify(body),
      signal: AbortSignal.timeout(15_000),
    };
    const request =
      opts.sign === false ? new Request(this.config.endpoint, init) : await this.#aws.sign(this.config.endpoint, init);
    const res = await this.#fetch(request);
    const text = await res.text();
    let json: Record<string, unknown> = {};
    try {
      json = text ? (JSON.parse(text) as Record<string, unknown>) : {};
    } catch {
      // not JSON: keep the raw text for the error message below
    }
    if (!res.ok) {
      const rawType = String(json.__type ?? json.code ?? "UnknownError");
      const message = String(json.message ?? json.Message ?? (text || res.statusText));
      throw new DynamoError(rawType.split("#").pop() ?? rawType, message, res.status);
    }
    return json as T;
  }

  /** Creates a single-table-style table: string partition key `PK`, string sort key `SK`. */
  createSingleTable(name: string): Promise<unknown> {
    return this.call("CreateTable", {
      TableName: name,
      BillingMode: "PAY_PER_REQUEST",
      AttributeDefinitions: [
        { AttributeName: "PK", AttributeType: "S" },
        { AttributeName: "SK", AttributeType: "S" },
      ],
      KeySchema: [
        { AttributeName: "PK", KeyType: "HASH" },
        { AttributeName: "SK", KeyType: "RANGE" },
      ],
    });
  }

  deleteTable(name: string): Promise<unknown> {
    return this.call("DeleteTable", { TableName: name });
  }

  /** Deletes a table and ignores "it is already gone". Any other error still throws. */
  async deleteTableIfExists(name: string): Promise<void> {
    try {
      await this.deleteTable(name);
    } catch (e) {
      if (!(e instanceof DynamoError && e.type === "ResourceNotFoundException")) throw e;
    }
  }

  putItem(table: string, item: Item, extra: Record<string, unknown> = {}): Promise<unknown> {
    return this.call("PutItem", { TableName: table, Item: item, ...extra });
  }

  /** Returns the item, or undefined when it does not exist. */
  async getItem(table: string, key: { PK: string; SK: string }): Promise<Item | undefined> {
    const out = await this.call<{ Item?: Item }>("GetItem", {
      TableName: table,
      Key: toItem(key),
      ConsistentRead: true,
    });
    return out.Item;
  }

  deleteItem(table: string, key: { PK: string; SK: string }): Promise<unknown> {
    return this.call("DeleteItem", { TableName: table, Key: toItem(key) });
  }

  query(table: string, params: Record<string, unknown>): Promise<QueryResult> {
    return this.call<QueryResult>("Query", { TableName: table, ConsistentRead: true, ...params });
  }
}

/** A name that is unique per call, so concurrent runs and reruns never collide: `news-test-<time>-<random>`. */
export function uniqueTableName(prefix = "news-test"): string {
  return `${prefix}-${Date.now().toString(36)}-${crypto.randomUUID().slice(0, 8)}`;
}

/**
 * Creates a throwaway single table, runs `fn`, and always deletes the table afterwards.
 * If `fn` throws, that error is the one you see even when the cleanup also fails.
 */
export async function withTempTable<T>(client: DynamoDbLocal, fn: (table: string) => Promise<T>): Promise<T> {
  const table = uniqueTableName();
  await client.createSingleTable(table);
  let result: T;
  try {
    result = await fn(table);
  } catch (error) {
    await client.deleteTableIfExists(table).catch(() => {});
    throw error;
  }
  await client.deleteTableIfExists(table);
  return result;
}
