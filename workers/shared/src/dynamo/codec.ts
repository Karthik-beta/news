// DynamoDB's typed JSON (`{ "S": "x" }`, `{ "N": "1" }`, …) to and from plain JSON values.
import type { Doc, Plain } from "./table";

export type AttributeValue =
  | { S: string }
  | { N: string }
  | { BOOL: boolean }
  | { NULL: true }
  | { L: AttributeValue[] }
  | { M: Record<string, AttributeValue> };
export type Item = Record<string, AttributeValue>;

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

export function toItem(doc: Doc): Item {
  const out: Item = {};
  for (const [k, v] of Object.entries(doc)) {
    // `undefined` never reaches DynamoDB: optional fields are simply left out.
    if (v !== undefined) out[k] = toAttribute(v);
  }
  return out;
}

export function fromAttribute(attr: AttributeValue): Plain {
  if ("S" in attr) return attr.S;
  if ("N" in attr) return Number(attr.N);
  if ("BOOL" in attr) return attr.BOOL;
  if ("NULL" in attr) return null;
  if ("L" in attr) return attr.L.map(fromAttribute);
  if ("M" in attr) return fromItem(attr.M);
  throw new Error(`unsupported DynamoDB attribute type: ${Object.keys(attr).join(",")}`);
}

export function fromItem(item: Item): Doc {
  const out: Doc = {};
  for (const [k, v] of Object.entries(item)) out[k] = fromAttribute(v);
  return out;
}
