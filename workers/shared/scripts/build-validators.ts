// Compiles the JSON Schemas in contracts/ into standalone validator functions (src/generated/validators.js).
//
// Workers cannot compile schemas at request time: `new Function` is not allowed in workerd, and compiling would spend
// the small CPU budget of a free-plan request. So the schemas are compiled here, ahead of time, and the generated
// file is committed. A unit test (validators.test.ts) fails when it is out of date: run `bun run validators` in
// workers/shared after changing any schema.
import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import Ajv2020 from "ajv/dist/2020.js";
import standaloneCode from "ajv/dist/standalone/index.js";

const CONTRACTS = fileURLToPath(new URL("../../../contracts", import.meta.url));
export const OUTPUT = fileURLToPath(new URL("../src/generated/validators.js", import.meta.url));
export const OUTPUT_TYPES = fileURLToPath(new URL("../src/generated/validators.d.ts", import.meta.url));
const BASE = "https://news.karthik.nz/contracts";

/** Exported validator name → schema reference. */
export const VALIDATORS: Record<string, string> = {
  validateMeta: `${BASE}/bundle/meta.schema.json`,
  validateManifest: `${BASE}/bundle/manifest.schema.json`,
  validateCuration: `${BASE}/cloud/curation.v1.schema.json`,
  validatePutPageRequest: `${BASE}/cloud/write-api.schema.json#/$defs/putPageRequest`,
  validateWithdrawRequest: `${BASE}/cloud/write-api.schema.json#/$defs/withdrawRequest`,
  validateIndexRequest: `${BASE}/cloud/write-api.schema.json#/$defs/indexRequest`,
  validateRollbackRequest: `${BASE}/cloud/write-api.schema.json#/$defs/rollbackRequest`,
  validateTaxonomyRequest: `${BASE}/cloud/write-api.schema.json#/$defs/taxonomyRequest`,
  validateRedirectsRequest: `${BASE}/cloud/write-api.schema.json#/$defs/redirectsRequest`,
  validateErrorBody: `${BASE}/cloud/write-api.schema.json#/$defs/error`,
  validatePutPageResponse: `${BASE}/cloud/write-api.schema.json#/$defs/putPageResponse`,
  validatePageOpResponse: `${BASE}/cloud/write-api.schema.json#/$defs/pageOpResponse`,
  validateCurationResponse: `${BASE}/cloud/write-api.schema.json#/$defs/curationResponse`,
  validateReportsResponse: `${BASE}/cloud/write-api.schema.json#/$defs/reportsResponse`,
  validateReportsCursor: `${BASE}/cloud/write-api.schema.json#/$defs/reportsCursor`,
  validateReportIntake: `${BASE}/cloud/report.schema.json#/$defs/intake`,
  validateReportRecord: `${BASE}/cloud/report.schema.json#/$defs/record`,
  validatePageItem: `${BASE}/cloud/dynamodb-items.schema.json#/$defs/pageRecord`,
  validateHubEntryItem: `${BASE}/cloud/dynamodb-items.schema.json#/$defs/hubEntry`,
  validateEntityItem: `${BASE}/cloud/dynamodb-items.schema.json#/$defs/entityRecord`,
  validateTaxonomyItem: `${BASE}/cloud/dynamodb-items.schema.json#/$defs/taxonomyNode`,
  validateRedirectItem: `${BASE}/cloud/dynamodb-items.schema.json#/$defs/redirect`,
  validatePathClaimItem: `${BASE}/cloud/dynamodb-items.schema.json#/$defs/pathClaim`,
  validatePageLockItem: `${BASE}/cloud/dynamodb-items.schema.json#/$defs/pageLock`,
  validateCurationItem: `${BASE}/cloud/dynamodb-items.schema.json#/$defs/curationRecord`,
  validateIdempotencyItem: `${BASE}/cloud/dynamodb-items.schema.json#/$defs/idempotencyRecord`,
  validateOperationResultItem: `${BASE}/cloud/dynamodb-items.schema.json#/$defs/operationResult`,
  validateSearchPagesInput: `${BASE}/cloud/mcp-tools.schema.json#/$defs/searchPagesInput`,
  validateSearchPagesOutput: `${BASE}/cloud/mcp-tools.schema.json#/$defs/searchPagesOutput`,
  validateGetPageInput: `${BASE}/cloud/mcp-tools.schema.json#/$defs/getPageInput`,
  validateGetPageOutput: `${BASE}/cloud/mcp-tools.schema.json#/$defs/getPageOutput`,
  validateListCategoriesInput: `${BASE}/cloud/mcp-tools.schema.json#/$defs/listCategoriesInput`,
  validateListCategoriesOutput: `${BASE}/cloud/mcp-tools.schema.json#/$defs/listCategoriesOutput`,
  validateGetEntityInput: `${BASE}/cloud/mcp-tools.schema.json#/$defs/getEntityInput`,
  validateGetEntityOutput: `${BASE}/cloud/mcp-tools.schema.json#/$defs/getEntityOutput`,
  validateGetDataInput: `${BASE}/cloud/mcp-tools.schema.json#/$defs/getDataInput`,
  validateGetDataOutput: `${BASE}/cloud/mcp-tools.schema.json#/$defs/getDataOutput`,
};

function schemaFiles(dir: string): string[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) return schemaFiles(path);
    return entry.name.endsWith(".schema.json") ? [path] : [];
  });
}

/** The generated module's source text. */
export function generate(): string {
  const ajv = new Ajv2020({
    code: { source: true, esm: true, lines: false },
    allErrors: true,
    strict: true,
    // `not: { required: [a, b] }` (at most one of two optional properties) names properties defined beside it.
    strictRequired: false,
    allowUnionTypes: true,
  });
  for (const file of schemaFiles(CONTRACTS).sort()) ajv.addSchema(JSON.parse(readFileSync(file, "utf8")));
  // Ajv's standalone output still `require`s two small runtime helpers even in ESM mode. Swap them for local
  // equivalents so the module is plain ESM with no dependency at all.
  const code = standaloneCode(ajv, VALIDATORS)
    .replace(/const (func\d+) = require\("ajv\/dist\/runtime\/ucs2length"\)\.default;/, "const $1 = ucs2length;")
    .replace(/const (func\d+) = require\("ajv\/dist\/runtime\/equal"\)\.default;/, "const $1 = deepEqual;");
  if (code.includes("require(")) throw new Error("generated validators still contain require(): add a replacement");
  return [
    "// GENERATED by workers/shared/scripts/build-validators.ts from contracts/**/*.schema.json. Do not edit.",
    "// biome-ignore-all lint: generated code",
    "// biome-ignore-all format: generated code",
    RUNTIME_HELPERS,
    code,
    "",
  ].join("\n");
}

/** Type declarations for the generated module. */
export function generateTypes(): string {
  return [
    "// GENERATED by workers/shared/scripts/build-validators.ts. Do not edit.",
    "export interface SchemaError {",
    "  instancePath: string;",
    "  schemaPath: string;",
    "  keyword: string;",
    "  params: Record<string, unknown>;",
    "  message?: string;",
    "}",
    "export interface Validator {",
    "  (data: unknown): boolean;",
    "  errors?: SchemaError[] | null;",
    "}",
    ...Object.keys(VALIDATORS).map((name) => `export declare const ${name}: Validator;`),
    "",
  ].join("\n");
}

/** Replacements for ajv/dist/runtime/{ucs2length,equal}: length in code points; deep equality of JSON values. */
const RUNTIME_HELPERS = `function ucs2length(str) { let n = 0; for (const _ of str) n++; return n; }
function deepEqual(a, b) {
  if (a === b) return true;
  if (typeof a !== "object" || typeof b !== "object" || a === null || b === null) return a !== a && b !== b;
  if (Array.isArray(a) !== Array.isArray(b)) return false;
  if (Array.isArray(a)) return a.length === b.length && a.every((v, i) => deepEqual(v, b[i]));
  const ka = Object.keys(a), kb = Object.keys(b);
  return ka.length === kb.length && ka.every((k) => Object.hasOwn(b, k) && deepEqual(a[k], b[k]));
}`;

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  writeFileSync(OUTPUT, generate());
  writeFileSync(OUTPUT_TYPES, generateTypes());
  console.log(`wrote ${OUTPUT} and ${OUTPUT_TYPES}`);
}
