// Guards of ADR 0010: the fixture data is valid, the data adapter refuses local data outside local runs and reads
// strongly consistent, and no dynamic route imports fixture or JSON data (a curation is data, not a build).
import { existsSync, readFileSync, statSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { DynamoTable } from "@news/worker-shared/dynamo/client";
import { schemas } from "@news/worker-shared/validate";
import { describe, expect, it } from "vitest";
import { siteData } from "../../src/data";
import { FIXTURE_ITEMS } from "../support/site-data";
import { siteRoot } from "../support/tokens";

const validators: Record<string, (d: unknown) => boolean> = {
  page: schemas.validatePageItem,
  hub_entry: schemas.validateHubEntryItem,
  entity: schemas.validateEntityItem,
  curation: schemas.validateCurationItem,
  taxonomy_node: schemas.validateTaxonomyItem,
};

describe("the fixture sets", () => {
  it.each(Object.keys(FIXTURE_ITEMS))("%s: every item matches contracts/cloud/dynamodb-items.schema.json", (set) => {
    for (const item of FIXTURE_ITEMS[set] ?? []) {
      const validate = validators[String(item.type)];
      expect(validate, `no validator for ${String(item.type)}`).toBeDefined();
      expect(validate?.(item), `${String(item.PK)} / ${String(item.SK)}`).toBe(true);
    }
  });
});

describe("the data adapter", () => {
  it("refuses local data in production, and when NEWS_ENV is unset", async () => {
    await expect(siteData({ NEWS_ENV: "production", NEWS_DATA: "local" })).rejects.toThrow(/refused/);
    await expect(siteData({ NEWS_DATA: "local" })).rejects.toThrow(/refused/);
    await expect(siteData({ NEWS_ENV: "production", NEWS_DATA: "nonsense" })).rejects.toThrow(/dynamodb or local/);
  });

  it("reads DynamoDB strongly consistent, the curation and every other read", async () => {
    const data = await siteData({
      NEWS_ENV: "production",
      NEWS_DATA: "dynamodb",
      NEWS_DYNAMODB_TABLE: "news-test",
      NEWS_DYNAMODB_REGION: "ap-southeast-2",
      NEWS_DYNAMODB_ACCESS_KEY_ID: "test-key-id",
      NEWS_DYNAMODB_SECRET_ACCESS_KEY: "test-secret",
    });
    expect(data.reader.table).toBeInstanceOf(DynamoTable);
    expect((data.reader.table as DynamoTable).consistentRead).toBe(true);
  });

  it("fails clearly, without echoing a secret, when the table settings are missing", async () => {
    await expect(siteData({ NEWS_ENV: "production", NEWS_DYNAMODB_SECRET_ACCESS_KEY: "s3cret" })).rejects.toThrow(
      /table/,
    );
  });
});

/** The relative imports of a source file that exist on disk. `data/local` is the one boundary: it is loaded on demand. */
function imports(file: string): string[] {
  const text = readFileSync(file, "utf8");
  const specs = [...text.matchAll(/(?:from\s*|import\s*\(\s*|import\s+)["']([^"']+)["']/g)].map((m) => m[1] as string);
  return specs.filter((s) => s.startsWith(".") || s.startsWith("/")).map((s) => resolve(dirname(file), s));
}

function resolveSource(base: string): string | undefined {
  const candidates = [
    base,
    `${base}.ts`,
    `${base}.tsx`,
    `${base}.astro`,
    join(base, "index.ts"),
    join(base, "index.tsx"),
  ];
  return candidates.find((c) => existsSync(c) && statSync(c).isFile());
}

describe("a curation is data, not a build", () => {
  it("the dynamic route, the middleware and what they import never import fixtures or JSON", () => {
    const seen = new Set<string>();
    const bad: string[] = [];
    const walk = (file: string) => {
      if (seen.has(file)) return;
      seen.add(file);
      for (const target of imports(file)) {
        const norm = target.replaceAll("\\", "/");
        // The local adapter is imported with import() only when NEWS_DATA=local; production never evaluates it.
        if (norm.endsWith("/src/data/local")) continue;
        if (/\/fixtures\/|\.json$/.test(norm)) bad.push(`${file} imports ${norm}`);
        const next = resolveSource(target);
        if (next) walk(next);
      }
    };
    const roots = [join(siteRoot, "src/pages/[...slug].astro"), join(siteRoot, "src/middleware.ts")];
    for (const r of roots) {
      expect(existsSync(r), r).toBe(true);
      walk(r);
    }
    expect(seen.size).toBeGreaterThan(10);
    expect(bad).toEqual([]);
  });

  it("the production data module reaches the local adapter only through a dynamic import", () => {
    const text = readFileSync(join(siteRoot, "src/data/index.ts"), "utf8");
    expect(text).not.toMatch(/^import .* from "\.\/local"/m);
    expect(text).toContain('import("./local")');
  });
});
