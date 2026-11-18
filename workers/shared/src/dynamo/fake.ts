// In-memory Table for unit tests. Behaves like DynamoDB for everything the Workers use (checked by contract.ts,
// which runs the same suite against DynamoDB Local), and records every call so a test can assert "wrote nothing".
import { type Cond, evaluate } from "./conditions";
import {
  BATCH_WRITE_MAX,
  type BatchOp,
  ConditionFailedError,
  type Doc,
  DynamoError,
  itemSize,
  type Key,
  keyOf,
  MAX_ITEM_BYTES,
  type QueryInput,
  type QueryOutput,
  type Table,
  ThrottledError,
  type TransactOp,
} from "./table";

export type FakeOp = "get" | "batchGet" | "put" | "delete" | "query" | "transact" | "batchWrite";

export interface FakeCall {
  op: FakeOp;
  keys: Key[];
}

const WRITES: ReadonlySet<FakeOp> = new Set(["put", "delete", "transact", "batchWrite"]);

const id = (k: Key) => `${k.PK}\u0000${k.SK}`;
const clone = <T>(v: T): T => structuredClone(v);

export class FakeTable implements Table {
  readonly #items = new Map<string, Doc>();
  /** Every call, in order. A batch write is one call per BATCH_WRITE_MAX items, as DynamoDB counts them. */
  readonly calls: FakeCall[] = [];
  /** Set to make the next N calls fail as throttled (tests of the 503 path). */
  throttleNext = 0;

  /** Calls that changed (or tried to change) data. */
  get writes(): FakeCall[] {
    return this.calls.filter((c) => WRITES.has(c.op));
  }

  /** A copy of every stored item, sorted by key. */
  dump(): Doc[] {
    return [...this.#items.values()].map(clone).sort((a, b) => (id(keyOf(a)) < id(keyOf(b)) ? -1 : 1));
  }

  /** Stores items directly, without recording a call (test setup). */
  seed(...items: Doc[]): void {
    for (const item of items) this.#items.set(id(keyOf(item)), clone(item));
  }

  #record(op: FakeOp, keys: Key[]): void {
    this.calls.push({ op, keys });
    if (this.throttleNext > 0) {
      this.throttleNext--;
      throw new ThrottledError();
    }
  }

  async get(key: Key): Promise<Doc | undefined> {
    this.#record("get", [key]);
    const item = this.#items.get(id(key));
    return item && clone(item);
  }

  async batchGet(keys: Key[]): Promise<Doc[]> {
    this.#record("batchGet", keys);
    const seen = new Set<string>();
    const out: Doc[] = [];
    for (const key of keys) {
      if (seen.has(id(key))) continue;
      seen.add(id(key));
      const item = this.#items.get(id(key));
      if (item) out.push(clone(item));
    }
    return out;
  }

  #checkPut(item: Doc): void {
    const key = keyOf(item);
    if (key.PK === "" || key.SK === "")
      throw new DynamoError("ValidationException", "key attributes must not be empty", 400);
    if (itemSize(item) > MAX_ITEM_BYTES)
      throw new DynamoError("ValidationException", "Item size has exceeded the maximum allowed size", 400);
  }

  async put(item: Doc, cond?: Cond): Promise<void> {
    this.#record("put", [keyOf(item)]);
    this.#checkPut(item);
    const key = id(keyOf(item));
    if (cond && !evaluate(cond, this.#items.get(key))) throw new ConditionFailedError();
    this.#items.set(key, clone(item));
  }

  async delete(key: Key, cond?: Cond): Promise<void> {
    this.#record("delete", [key]);
    if (cond && !evaluate(cond, this.#items.get(id(key)))) throw new ConditionFailedError();
    this.#items.delete(id(key));
  }

  async query(input: QueryInput): Promise<QueryOutput> {
    this.#record("query", [{ PK: input.pk, SK: input.start?.SK ?? "" }]);
    const forward = input.forward ?? true;
    let items = [...this.#items.values()].filter((item) => item.PK === input.pk && matchesSk(item.SK as string, input));
    items.sort((a, b) => compareSk(a.SK as string, b.SK as string));
    if (!forward) items.reverse();
    if (input.start) {
      const startSk = input.start.SK;
      items = items.filter((item) => (forward ? (item.SK as string) > startSk : (item.SK as string) < startSk));
    }
    const limit = input.limit ?? Number.POSITIVE_INFINITY;
    const page = items.slice(0, limit);
    // Like DynamoDB: a page cut by Limit carries a LastEvaluatedKey even when nothing follows it.
    const next = items.length >= limit && page.length > 0 ? keyOf(page[page.length - 1] as Doc) : undefined;
    return { items: page.map(clone), ...(next ? { next } : {}) };
  }

  async batchWrite(ops: BatchOp[]): Promise<void> {
    for (let i = 0; i < ops.length; i += BATCH_WRITE_MAX) {
      const chunk = ops.slice(i, i + BATCH_WRITE_MAX);
      const keys = chunk.map((op) => ("put" in op ? keyOf(op.put) : op.delete));
      this.#record("batchWrite", keys);
      if (new Set(keys.map(id)).size !== keys.length)
        throw new DynamoError("ValidationException", "Provided list of item keys contains duplicates", 400);
      for (const op of chunk) if ("put" in op) this.#checkPut(op.put);
      for (const op of chunk) {
        if ("put" in op) this.#items.set(id(keyOf(op.put)), clone(op.put));
        else this.#items.delete(id(op.delete));
      }
    }
  }

  async transact(ops: TransactOp[]): Promise<void> {
    const keys = ops.map(opKey);
    this.#record("transact", keys);
    if (ops.length === 0 || ops.length > 100)
      throw new DynamoError("ValidationException", "a transaction needs 1 to 100 operations", 400);
    if (new Set(keys.map(id)).size !== keys.length)
      throw new DynamoError(
        "ValidationException",
        "Transaction request cannot include multiple operations on one item",
        400,
      );
    for (const op of ops) if ("put" in op) this.#checkPut(op.put);
    for (const op of ops) {
      if (op.cond && !evaluate(op.cond, this.#items.get(id(opKey(op))))) {
        throw new ConditionFailedError("Transaction cancelled: a condition failed");
      }
    }
    for (const op of ops) {
      if ("put" in op) this.#items.set(id(keyOf(op.put)), clone(op.put));
      else if ("delete" in op) this.#items.delete(id(op.delete));
    }
  }
}

const opKey = (op: TransactOp): Key => ("put" in op ? keyOf(op.put) : "delete" in op ? op.delete : op.check);

// DynamoDB compares string sort keys by their UTF-8 bytes. For the BMP that equals UTF-16 code-unit order, which is
// what JavaScript's `<` does; keys here are ASCII.
function compareSk(a: string, b: string): number {
  return a < b ? -1 : a > b ? 1 : 0;
}

function matchesSk(sk: string, input: QueryInput): boolean {
  const c = input.sk;
  if (!c) return true;
  if ("beginsWith" in c) return sk.startsWith(c.beginsWith);
  if ("eq" in c) return sk === c.eq;
  return sk >= c.between[0] && sk <= c.between[1];
}
