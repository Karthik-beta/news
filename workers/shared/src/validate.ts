// Runs a precompiled validator and turns its errors into short, safe issue lists for API error bodies.
import type { Validator } from "./generated/validators.js";

export type { SchemaError, Validator } from "./generated/validators.js";

export * as schemas from "./generated/validators.js";

export interface Issue {
  code: string;
  message: string;
  path?: string;
}

const MAX_ISSUES = 20;

export function issuesOf(validator: Validator): Issue[] {
  return (validator.errors ?? []).slice(0, MAX_ISSUES).map((e) => ({
    code: `schema.${e.keyword}`,
    message: e.message ?? e.keyword,
    // The JSON Pointer of the offending value; never the value itself, which may be large or hostile.
    path: e.instancePath || "/",
  }));
}

export type Checked<T> = { ok: true; value: T } | { ok: false; issues: Issue[] };

export function check<T>(validator: Validator, data: unknown): Checked<T> {
  return validator(data) ? { ok: true, value: data as T } : { ok: false, issues: issuesOf(validator) };
}
