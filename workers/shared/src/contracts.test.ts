// The contracts folder stays consistent: every $ref in the schemas and OpenAPI files points at something that exists.
import { readdirSync, readFileSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const CONTRACTS = fileURLToPath(new URL("../../../contracts", import.meta.url));

const files = (dir: string): string[] =>
  readdirSync(dir, { withFileTypes: true }).flatMap((e) =>
    e.isDirectory() ? files(join(dir, e.name)) : e.name.endsWith(".json") ? [join(dir, e.name)] : [],
  );

function refs(node: unknown, out: string[] = []): string[] {
  if (Array.isArray(node)) for (const n of node) refs(n, out);
  else if (node && typeof node === "object") {
    for (const [k, v] of Object.entries(node)) {
      if (k === "$ref" && typeof v === "string") out.push(v);
      else refs(v, out);
    }
  }
  return out;
}

function resolvePointer(doc: unknown, pointer: string): unknown {
  if (pointer === "" || pointer === "/") return doc;
  return pointer
    .slice(1)
    .split("/")
    .map((p) => p.replaceAll("~1", "/").replaceAll("~0", "~"))
    .reduce<unknown>(
      (node, key) => (node && typeof node === "object" ? (node as Record<string, unknown>)[key] : undefined),
      doc,
    );
}

describe("contracts/", () => {
  const all = files(CONTRACTS);

  it("has the expected files", () => {
    const names = all.map((f) => relative(CONTRACTS, f).replaceAll("\\", "/")).sort();
    expect(names).toEqual(
      expect.arrayContaining([
        "bundle/manifest.schema.json",
        "bundle/meta.schema.json",
        "cloud/curation.v1.schema.json",
        "cloud/dynamodb-items.schema.json",
        "cloud/mcp-tools.schema.json",
        "cloud/report.schema.json",
        "cloud/search-api.openapi.json",
        "cloud/write-api.openapi.json",
        "cloud/write-api.schema.json",
      ]),
    );
  });

  it.each(all.map((f) => [relative(CONTRACTS, f), f]))("%s: every $ref resolves", (_name, file) => {
    const doc = JSON.parse(readFileSync(file, "utf8"));
    for (const ref of refs(doc)) {
      const [path, pointer = ""] = ref.split("#");
      const target = path ? JSON.parse(readFileSync(join(dirname(file), path), "utf8")) : doc;
      expect(resolvePointer(target, pointer), ref).toBeDefined();
    }
  });
});
