import { readFileSync } from "node:fs";
import { join } from "node:path";
import Ajv2020 from "ajv/dist/2020";
import { describe, expect, it } from "vitest";
import { contractSchemas } from "../../scripts/gen-contracts";
import { fixtureBundles } from "../../src/renderer/fixtures/bundles/vite";
import { repoRoot } from "../support/tokens";

// contracts/bundle/*.schema.json: generated from src/renderer/bundle/schema.ts, committed, and checked here with
// an independent JSON Schema validator (Ajv), so the Rust CLI and the renderer agree on what a bundle is.
const read = (p: string) => JSON.parse(readFileSync(join(repoRoot, p), "utf8"));
const dataSchema = read("contracts/bundle/data.schema.json");
const sourcesSchema = read("contracts/bundle/sources.schema.json");
const ajv = new Ajv2020({ allErrors: true, strict: false });
const validateData = ajv.compile(dataSchema);
const validateSources = ajv.compile(sourcesSchema);

describe("bundle contracts", () => {
  it("are JSON Schema 2020-12 and match the zod schemas (run `bun run contracts` after a change)", () => {
    for (const [path, text] of Object.entries(contractSchemas())) {
      expect(readFileSync(join(repoRoot, path), "utf8").replaceAll("\r\n", "\n"), path).toBe(text);
    }
    expect(dataSchema.$schema).toBe("https://json-schema.org/draft/2020-12/schema");
    expect(sourcesSchema.$schema).toBe("https://json-schema.org/draft/2020-12/schema");
  });

  it.each(Object.keys(fixtureBundles))("accept the %s fixture bundle", (name) => {
    const b = fixtureBundles[name as keyof typeof fixtureBundles];
    expect(validateData(b.data), JSON.stringify(validateData.errors)).toBe(true);
    expect(validateSources(b.sources), JSON.stringify(validateSources.errors)).toBe(true);
  });

  const base = () => structuredClone(fixtureBundles["apple-fy2024"].data) as Record<string, Record<string, unknown>>;
  const point = (d: Record<string, Record<string, unknown>>) =>
    (d.points as Record<string, Record<string, unknown>>).dp_aapl_net_sales_fy2024 as Record<string, unknown>;

  it("reject a reference with the wrong shape", () => {
    const d = base();
    (d.points as Record<string, unknown>).DP_Upper = point(d);
    expect(validateData(d)).toBe(false);
    const e = base();
    (e.points as Record<string, unknown>)["dp_ends_"] = point(e);
    expect(validateData(e)).toBe(false);
  });

  it("reject a value that is a number, not an exact decimal string", () => {
    for (const bad of [391035, "391,035", "3.9e5", "+1", "01"]) {
      const d = base();
      point(d).value = bad;
      expect(validateData(d), String(bad)).toBe(false);
    }
  });

  it("reject a currency that is not an ISO 4217 code, and unknown fields", () => {
    const d = base();
    point(d).currency = "US$";
    expect(validateData(d)).toBe(false);
    const e = base();
    point(e).converted_value = "1";
    expect(validateData(e)).toBe(false);
  });

  it("accept a missing value as null", () => {
    const d = base();
    point(d).value = null;
    point(d).value_text = null;
    expect(validateData(d)).toBe(true);
  });

  it("reject a source without a licence decision", () => {
    const s = structuredClone(fixtureBundles["apple-fy2024"].sources) as {
      sources: Record<string, { licence: object }>;
    };
    (s.sources.src_aapl_10k_2024 as { licence: object }).licence = { name: "Public filing" };
    expect(validateSources(s)).toBe(false);
  });
});
