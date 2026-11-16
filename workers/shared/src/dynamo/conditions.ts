// Write conditions as a tiny expression tree. The real client compiles a tree to a DynamoDB ConditionExpression;
// the fake evaluates the same tree with DynamoDB's semantics (a comparison on a missing attribute is false).
import type { Doc, Plain } from "./table";

export type Cond =
  | { op: "not_exists" }
  | { op: "exists" }
  | { op: "eq" | "lt" | "le"; attr: string; value: string | number | boolean }
  | { op: "and" | "or"; conds: Cond[] };

export const notExists = (): Cond => ({ op: "not_exists" });
export const exists = (): Cond => ({ op: "exists" });
export const eq = (attr: string, value: string | number | boolean): Cond => ({ op: "eq", attr, value });
export const lt = (attr: string, value: number | string): Cond => ({ op: "lt", attr, value });
export const le = (attr: string, value: number | string): Cond => ({ op: "le", attr, value });
export const and = (...conds: Cond[]): Cond => ({ op: "and", conds });
export const or = (...conds: Cond[]): Cond => ({ op: "or", conds });

const ATTR_NAME = /^[A-Za-z_][A-Za-z0-9_]{0,63}$/;

export interface CompiledCondition {
  ConditionExpression: string;
  ExpressionAttributeNames: Record<string, string>;
  ExpressionAttributeValues: Record<string, Plain>;
}

const SYMBOL = { eq: "=", lt: "<", le: "<=" } as const;

/** Compiles to DynamoDB expression syntax. Attribute names and values are always placeholders, never inlined. */
export function compile(cond: Cond): CompiledCondition {
  const names: Record<string, string> = {};
  const values: Record<string, Plain> = {};
  let n = 0;
  const name = (attr: string) => {
    if (!ATTR_NAME.test(attr)) throw new Error(`invalid attribute name in condition: ${attr}`);
    const placeholder = `#${attr}`;
    names[placeholder] = attr;
    return placeholder;
  };
  const walk = (c: Cond): string => {
    switch (c.op) {
      case "not_exists":
        return `attribute_not_exists(${name("PK")})`;
      case "exists":
        return `attribute_exists(${name("PK")})`;
      case "and":
      case "or": {
        if (c.conds.length === 0) throw new Error(`empty '${c.op}' condition`);
        return `(${c.conds.map(walk).join(c.op === "and" ? " AND " : " OR ")})`;
      }
      default: {
        const placeholder = `:v${n++}`;
        values[placeholder] = c.value;
        return `${name(c.attr)} ${SYMBOL[c.op]} ${placeholder}`;
      }
    }
  };
  return { ConditionExpression: walk(cond), ExpressionAttributeNames: names, ExpressionAttributeValues: values };
}

/** DynamoDB's own evaluation: `item` is the current item, or undefined when there is none. */
export function evaluate(cond: Cond, item: Doc | undefined): boolean {
  switch (cond.op) {
    case "not_exists":
      return item === undefined;
    case "exists":
      return item !== undefined;
    case "and":
      return cond.conds.every((c) => evaluate(c, item));
    case "or":
      return cond.conds.some((c) => evaluate(c, item));
    default: {
      const current = item?.[cond.attr];
      // A missing attribute, or one of another type, makes the comparison false (DynamoDB semantics).
      if (current === undefined || typeof current !== typeof cond.value) return false;
      const a = current as string | number | boolean;
      const b = cond.value;
      if (cond.op === "eq") return a === b;
      if (typeof a === "boolean") return false;
      return cond.op === "lt" ? a < b : a <= b;
    }
  }
}
