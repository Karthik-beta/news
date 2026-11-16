// The small DynamoDB surface the Workers use, as an interface with two implementations:
//   DynamoTable (client.ts)  signed HTTPS calls to DynamoDB (or DynamoDB Local in tests)
//   FakeTable   (fake.ts)    in memory, for unit tests
// The same contract suite (contract.ts) runs against both, so the fake cannot drift from the real service.
import type { Cond } from "./conditions";

/** A JSON value as stored in an item. */
export type Plain = string | number | boolean | null | Plain[] | { [key: string]: Plain };
/** One decoded item. Every item has the string keys PK and SK. */
export type Doc = { [key: string]: Plain };

export interface Key {
  PK: string;
  SK: string;
}

export type SkCondition = { beginsWith: string } | { between: [string, string] } | { eq: string };

export interface QueryInput {
  pk: string;
  sk?: SkCondition;
  /** Items per page (DynamoDB stops earlier at 1 MB). */
  limit?: number;
  /** false: newest (largest sort key) first. Default true. */
  forward?: boolean;
  /** `next` from the previous page. */
  start?: Key;
}

export interface QueryOutput {
  items: Doc[];
  /** Present when more items may follow. */
  next?: Key;
}

export type TransactOp = { put: Doc; cond?: Cond } | { delete: Key; cond?: Cond } | { check: Key; cond: Cond };

/** One unconditional write in a batch. */
export type BatchOp = { put: Doc } | { delete: Key };

/** DynamoDB's BatchWriteItem takes at most 25 writes per call. */
export const BATCH_WRITE_MAX = 25;

export interface Table {
  get(key: Key): Promise<Doc | undefined>;
  /** Items that exist, in no particular order. Any number of keys: split into calls of 100. */
  batchGet(keys: Key[]): Promise<Doc[]>;
  /** Throws ConditionFailedError when `cond` is false for the current item (or for no item). */
  put(item: Doc, cond?: Cond): Promise<void>;
  delete(key: Key, cond?: Cond): Promise<void>;
  query(input: QueryInput): Promise<QueryOutput>;
  /** All or nothing, at most 100 operations, one per item. Throws ConditionFailedError if any condition fails. */
  transact(ops: TransactOp[]): Promise<void>;
  /**
   * Unconditional puts and deletes, BATCH_WRITE_MAX per call (one subrequest each), not atomic. Keys within one call
   * must differ. Writes DynamoDB leaves unprocessed are retried twice, then the batch fails as ThrottledError.
   */
  batchWrite(ops: BatchOp[]): Promise<void>;
}

export class ConditionFailedError extends Error {
  constructor(message = "The conditional request failed") {
    super(message);
    this.name = "ConditionFailedError";
  }
}

/** DynamoDB refused because the table's capacity is used up. Safe to retry later. */
export class ThrottledError extends Error {
  constructor(message = "DynamoDB throttled the request") {
    super(message);
    this.name = "ThrottledError";
  }
}

/**
 * A write whose outcome is unknown: the request was sent but no answer came back (timeout, dropped connection, a 5xx
 * from DynamoDB). It may or may not have been applied, so the caller must not treat it as "not written".
 */
export class OutcomeUnknownError extends Error {
  constructor(message = "the DynamoDB write may or may not have been applied") {
    super(message);
    this.name = "OutcomeUnknownError";
  }
}

/**
 * The Worker ran out of subrequests (the free plan allows 50 external ones per request): the call was never sent, so
 * nothing was written by it. An ordinary failure, not an unknown outcome.
 */
export class SubrequestLimitError extends Error {
  constructor(message = "the Worker's subrequest limit was reached; the call was not sent") {
    super(message);
    this.name = "SubrequestLimitError";
  }
}

/** Any other DynamoDB error. `type` is the short exception name, e.g. ValidationException. */
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

function toPlain(value: unknown, path: string): Plain {
  if (value === null || typeof value === "string" || typeof value === "boolean") return value;
  if (typeof value === "number") {
    if (!Number.isFinite(value)) throw new TypeError(`${path}: ${value} cannot be stored`);
    return value;
  }
  if (Array.isArray(value)) return value.map((v, i) => toPlain(v, `${path}/${i}`));
  const proto = typeof value === "object" ? Object.getPrototypeOf(value) : undefined;
  if (proto === Object.prototype || proto === null) return toDoc(value as object, path);
  throw new TypeError(`${path}: a ${typeof value} is not a JSON value`);
}

/**
 * A typed record (PageRecord, HubEntry, …) as a Doc, checked value by value instead of cast: undefined fields are left
 * out (as DynamoDB would), and anything that is not plain JSON (a Date, a function, NaN) is refused.
 */
export function toDoc(record: object, path = ""): Doc {
  const doc: Doc = {};
  for (const [k, v] of Object.entries(record))
    if (v !== undefined)
      Object.defineProperty(doc, k, {
        value: toPlain(v, `${path}/${k}`),
        enumerable: true,
        writable: true,
        configurable: true,
      });
  return doc;
}

export const keyOf = (item: Doc): Key => {
  if (typeof item.PK !== "string" || typeof item.SK !== "string") throw new Error("item has no string PK and SK");
  return { PK: item.PK, SK: item.SK };
};

/** DynamoDB's maximum item size. */
export const MAX_ITEM_BYTES = 400 * 1024;

const utf8 = new TextEncoder();

/**
 * Item size the way DynamoDB counts it, closely enough to stay under the limit: attribute names and string values in
 * UTF-8 bytes, numbers about 1 byte per 2 digits plus 1, and 3 bytes of overhead per list or map plus 1 per element.
 */
export function itemSize(doc: Doc): number {
  let size = 0;
  for (const [name, value] of Object.entries(doc)) size += utf8.encode(name).length + valueSize(value);
  return size;
}

function valueSize(value: Plain): number {
  if (value === null || typeof value === "boolean") return 1;
  if (typeof value === "string") return utf8.encode(value).length;
  if (typeof value === "number") return Math.ceil(String(Math.abs(value)).replace(/[.-]/g, "").length / 2) + 1;
  if (Array.isArray(value)) return 3 + value.reduce<number>((sum, v) => sum + 1 + valueSize(v), 0);
  return 3 + Object.entries(value).reduce((sum, [k, v]) => sum + 1 + utf8.encode(k).length + valueSize(v), 0);
}
