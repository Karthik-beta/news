// DynamoDB over HTTPS with SigV4-signed requests (Technical Design: "DynamoDB is called from Workers over HTTPS with
// signed requests"). fetch-based, no AWS SDK: small bundle, little CPU per request.
import { AwsClient } from "aws4fetch";
import { type AttributeValue, fromItem, type Item, toItem } from "./codec";
import { type Cond, compile } from "./conditions";
import {
  ConditionFailedError,
  type Doc,
  DynamoError,
  type Key,
  OutcomeUnknownError,
  type QueryInput,
  type QueryOutput,
  type Table,
  ThrottledError,
  type TransactOp,
} from "./table";

export interface DynamoTableConfig {
  table: string;
  region: string;
  accessKeyId: string;
  secretAccessKey: string;
  /** Default: https://dynamodb.<region>.amazonaws.com. Tests point it at DynamoDB Local on loopback. */
  endpoint?: string;
  fetch?: (request: Request) => Promise<Response>;
  /** Per-call timeout. Default 8 s, well inside a Worker request. */
  timeoutMs?: number;
}

const LOOPBACK = new Set(["127.0.0.1", "localhost", "[::1]"]);
const REGION = /^[a-z]{2}(?:-[a-z]+)+-[0-9]$/;
const TABLE = /^[A-Za-z0-9_.-]{3,255}$/;
const WRITES = new Set(["PutItem", "DeleteItem", "TransactWriteItems"]);
const THROTTLE_TYPES = new Set([
  "ProvisionedThroughputExceededException",
  "ThrottlingException",
  "RequestLimitExceeded",
  "LimitExceededException",
]);

/**
 * The endpoint to call, checked: either the region's official HTTPS endpoint or a loopback address (DynamoDB Local).
 * Anything else is refused, so a bad setting can never send signed requests to an arbitrary host.
 */
export function resolveEndpoint(region: string, endpoint?: string): string {
  if (!REGION.test(region)) throw new Error(`invalid AWS region: ${JSON.stringify(region)}`);
  const official = `https://dynamodb.${region}.amazonaws.com`;
  if (!endpoint || endpoint.trim() === "") return official;
  let url: URL;
  try {
    url = new URL(endpoint.trim());
  } catch {
    throw new Error("DynamoDB endpoint is not a valid URL");
  }
  const origin = url.origin;
  if (origin === official) return official;
  if (LOOPBACK.has(url.hostname) && (url.protocol === "http:" || url.protocol === "https:")) return origin;
  throw new Error(`refusing DynamoDB endpoint ${origin}: only ${official} or a loopback address is allowed`);
}

export class DynamoTable implements Table {
  readonly table: string;
  readonly endpoint: string;
  readonly #aws: AwsClient;
  readonly #fetch: (request: Request) => Promise<Response>;
  readonly #timeoutMs: number;

  constructor(config: DynamoTableConfig) {
    if (!TABLE.test(config.table)) throw new Error("invalid DynamoDB table name");
    if (!config.accessKeyId || !config.secretAccessKey) throw new Error("DynamoDB credentials are not set");
    this.table = config.table;
    this.endpoint = resolveEndpoint(config.region, config.endpoint);
    this.#fetch = config.fetch ?? ((r) => fetch(r));
    this.#timeoutMs = config.timeoutMs ?? 8000;
    // retries: 0 — the caller decides. A throttled publish returns 503 and the laptop retries with its idempotency key.
    this.#aws = new AwsClient({
      accessKeyId: config.accessKeyId,
      secretAccessKey: config.secretAccessKey,
      region: config.region,
      service: "dynamodb",
      retries: 0,
    });
  }

  async #call<T>(operation: string, body: object): Promise<T> {
    const request = await this.#aws.sign(this.endpoint, {
      method: "POST",
      headers: { "content-type": "application/x-amz-json-1.0", "x-amz-target": `DynamoDB_20120810.${operation}` },
      body: JSON.stringify(body),
      signal: AbortSignal.timeout(this.#timeoutMs),
    });
    const write = WRITES.has(operation);
    let res: Response;
    let text: string;
    try {
      res = await this.#fetch(request);
      text = await res.text();
    } catch (e) {
      // Sent, but no answer: a write may have been applied. Reads are simply failed.
      if (write) throw new OutcomeUnknownError(`${operation}: ${e instanceof Error ? e.name : "no response"}`);
      throw e;
    }
    let json: Record<string, unknown> = {};
    try {
      json = text ? (JSON.parse(text) as Record<string, unknown>) : {};
    } catch {
      // not JSON; handled below
    }
    if (res.ok) return json as T;
    const rawType = String(json.__type ?? "UnknownError");
    const type = rawType.split("#").pop() ?? rawType;
    // The message is DynamoDB's own text; it never contains our credentials.
    const message = String(json.message ?? json.Message ?? res.statusText).slice(0, 500);
    if (type === "ConditionalCheckFailedException") throw new ConditionFailedError();
    if (type === "TransactionCanceledException") {
      const reasons = (json.CancellationReasons as Array<{ Code?: string }> | undefined) ?? [];
      if (reasons.some((r) => r.Code === "ConditionalCheckFailed"))
        throw new ConditionFailedError("Transaction cancelled: a condition failed");
      if (reasons.some((r) => r.Code === "ThrottlingError" || r.Code === "ProvisionedThroughputExceeded"))
        throw new ThrottledError();
    }
    if (THROTTLE_TYPES.has(type)) throw new ThrottledError();
    // A 5xx (InternalServerError, ServiceUnavailable) on a write does not say whether it was applied.
    if (write && res.status >= 500) throw new OutcomeUnknownError(`${operation}: ${type} (HTTP ${res.status})`);
    throw new DynamoError(type, message, res.status);
  }

  #conditionParams(cond?: Cond): Record<string, unknown> {
    if (!cond) return {};
    const c = compile(cond);
    return {
      ConditionExpression: c.ConditionExpression,
      ExpressionAttributeNames: c.ExpressionAttributeNames,
      ...(Object.keys(c.ExpressionAttributeValues).length > 0
        ? { ExpressionAttributeValues: toItem(c.ExpressionAttributeValues) }
        : {}),
    };
  }

  async get(key: Key): Promise<Doc | undefined> {
    const out = await this.#call<{ Item?: Item }>("GetItem", {
      TableName: this.table,
      Key: toItem({ ...key }),
      ConsistentRead: true,
    });
    return out.Item ? fromItem(out.Item) : undefined;
  }

  async batchGet(keys: Key[]): Promise<Doc[]> {
    const unique = [...new Map(keys.map((k) => [`${k.PK}\u0000${k.SK}`, k])).values()];
    const out: Doc[] = [];
    for (let i = 0; i < unique.length; i += 100) {
      let request: Record<string, unknown> = {
        [this.table]: { Keys: unique.slice(i, i + 100).map((k) => toItem({ ...k })), ConsistentRead: true },
      };
      // Unprocessed keys come back when capacity runs short; retry them a few times, then give up as throttled.
      for (let attempt = 0; ; attempt++) {
        const res = await this.#call<{
          Responses?: Record<string, Item[]>;
          UnprocessedKeys?: Record<string, { Keys: Item[] }>;
        }>("BatchGetItem", { RequestItems: request });
        for (const item of res.Responses?.[this.table] ?? []) out.push(fromItem(item));
        const rest = res.UnprocessedKeys?.[this.table];
        if (!rest || rest.Keys.length === 0) break;
        if (attempt >= 3) throw new ThrottledError("DynamoDB left keys unprocessed");
        await new Promise((r) => setTimeout(r, 50 * 2 ** attempt));
        request = { [this.table]: { ...rest, ConsistentRead: true } };
      }
    }
    return out;
  }

  async put(item: Doc, cond?: Cond): Promise<void> {
    await this.#call("PutItem", { TableName: this.table, Item: toItem(item), ...this.#conditionParams(cond) });
  }

  async delete(key: Key, cond?: Cond): Promise<void> {
    await this.#call("DeleteItem", { TableName: this.table, Key: toItem({ ...key }), ...this.#conditionParams(cond) });
  }

  async query(input: QueryInput): Promise<QueryOutput> {
    const values: Record<string, AttributeValue> = { ":pk": { S: input.pk } };
    let expression = "#PK = :pk";
    const sk = input.sk;
    if (sk && "beginsWith" in sk) {
      expression += " AND begins_with(#SK, :sk)";
      values[":sk"] = { S: sk.beginsWith };
    } else if (sk && "eq" in sk) {
      expression += " AND #SK = :sk";
      values[":sk"] = { S: sk.eq };
    } else if (sk) {
      expression += " AND #SK BETWEEN :a AND :b";
      values[":a"] = { S: sk.between[0] };
      values[":b"] = { S: sk.between[1] };
    }
    const res = await this.#call<{ Items?: Item[]; LastEvaluatedKey?: Item }>("Query", {
      TableName: this.table,
      KeyConditionExpression: expression,
      ExpressionAttributeNames: sk ? { "#PK": "PK", "#SK": "SK" } : { "#PK": "PK" },
      ExpressionAttributeValues: values,
      ScanIndexForward: input.forward ?? true,
      ConsistentRead: true,
      ...(input.limit ? { Limit: input.limit } : {}),
      ...(input.start ? { ExclusiveStartKey: toItem({ ...input.start }) } : {}),
    });
    const items = (res.Items ?? []).map(fromItem);
    const last = res.LastEvaluatedKey ? fromItem(res.LastEvaluatedKey) : undefined;
    return {
      items,
      ...(last ? { next: { PK: String(last.PK), SK: String(last.SK) } } : {}),
    };
  }

  async transact(ops: TransactOp[]): Promise<void> {
    const items = ops.map((op) => {
      if ("put" in op)
        return { Put: { TableName: this.table, Item: toItem(op.put), ...this.#conditionParams(op.cond) } };
      if ("delete" in op)
        return { Delete: { TableName: this.table, Key: toItem({ ...op.delete }), ...this.#conditionParams(op.cond) } };
      return {
        ConditionCheck: { TableName: this.table, Key: toItem({ ...op.check }), ...this.#conditionParams(op.cond) },
      };
    });
    await this.#call("TransactWriteItems", { TransactItems: items });
  }
}

/** Builds the table from Worker bindings. Throws (without echoing any secret) when a setting is missing. */
export function dynamoTableFromEnv(env: {
  NEWS_DYNAMODB_TABLE?: string;
  NEWS_DYNAMODB_REGION?: string;
  NEWS_DYNAMODB_ENDPOINT?: string;
  NEWS_DYNAMODB_ACCESS_KEY_ID?: string;
  NEWS_DYNAMODB_SECRET_ACCESS_KEY?: string;
}): DynamoTable {
  return new DynamoTable({
    table: env.NEWS_DYNAMODB_TABLE ?? "",
    region: env.NEWS_DYNAMODB_REGION ?? "",
    endpoint: env.NEWS_DYNAMODB_ENDPOINT,
    accessKeyId: env.NEWS_DYNAMODB_ACCESS_KEY_ID ?? "",
    secretAccessKey: env.NEWS_DYNAMODB_SECRET_ACCESS_KEY ?? "",
  });
}
