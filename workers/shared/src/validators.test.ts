import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { generate, generateTypes, OUTPUT, OUTPUT_TYPES } from "../scripts/build-validators";
import { makeBundle, makeCuration, makeMeta } from "./testing/fixtures";
import { check, schemas } from "./validate";

describe("precompiled validators", () => {
  it("are up to date with contracts/ (run `bun run validators` in workers/shared if this fails)", () => {
    expect(readFileSync(OUTPUT, "utf8")).toBe(generate());
    expect(readFileSync(OUTPUT_TYPES, "utf8")).toBe(generateTypes());
  });

  it("contain no runtime code generation, which workerd forbids", () => {
    const code = readFileSync(OUTPUT, "utf8");
    expect(code).not.toMatch(/new Function|\beval\(|require\(/);
  });
});

describe("meta.v1", () => {
  it("accepts the fixture", () => {
    expect(check(schemas.validateMeta, makeMeta())).toEqual({ ok: true, value: makeMeta() });
  });

  it.each([
    ["no lang", { lang: undefined }],
    ["unknown field", { extra: 1 }],
    ["'#' in the ID", { id: "a#b" }],
    ["category in the path", { path: "/economy/2026/11/x" }],
    ["upper-case slug", { categories: ["Economy"] }],
    ["no categories", { categories: [] }],
    ["local time", { published_at: "2026-11-10T12:00:00+05:30" }],
    ["tier 4", { tier: 4 }],
    ["unknown page type", { page_type: "opinion" }],
    ["chart ID that is not a file name", { chart_ids: ["Chart One"] }],
  ])("refuses %s", (_label, change) => {
    const meta = { ...makeMeta(), ...change } as Record<string, unknown>;
    for (const [k, v] of Object.entries(change)) if (v === undefined) delete meta[k];
    const r = check(schemas.validateMeta, meta);
    expect(r.ok).toBe(false);
  });

  it("reports the path of the bad value, never the value", () => {
    const r = check(schemas.validateMeta, { ...makeMeta(), title: "<script>".repeat(100) });
    expect(r.ok).toBe(false);
    if (!r.ok) {
      expect(r.issues[0]).toMatchObject({ code: "schema.maxLength", path: "/title" });
      expect(JSON.stringify(r.issues)).not.toContain("<script>");
    }
  });
});

describe("put-page request", () => {
  it("accepts a fixture bundle", async () => {
    expect(check(schemas.validatePutPageRequest, await makeBundle()).ok).toBe(true);
  });

  it("accepts a chart's JSON download beside its CSV", async () => {
    const bundle = await makeBundle();
    const files = { ...bundle.files, "charts/cpi-trend.json": "{}\n" };
    expect(check(schemas.validatePutPageRequest, { ...bundle, files }).ok).toBe(true);
  });

  it.each(["../escape.html", "index.php", "charts/../x.csv", "CHARTS/a.csv", "charts/a.svg"])(
    "refuses the file name %s",
    async (name) => {
      const bundle = await makeBundle();
      expect(check(schemas.validatePutPageRequest, { ...bundle, files: { ...bundle.files, [name]: "x" } }).ok).toBe(
        false,
      );
    },
  );
});

describe("curation.v1", () => {
  it("accepts the fixture", () => {
    expect(check(schemas.validateCuration, makeCuration()).ok).toBe(true);
  });

  it("refuses a slot with both a figure and a chart", () => {
    const c = makeCuration();
    c.slots.side[0] = { page_id: "x", figure_id: "a", chart_id: "b" };
    expect(check(schemas.validateCuration, c).ok).toBe(false);
  });

  it("refuses a draft whose own checks failed, and a rationale field", () => {
    expect(check(schemas.validateCuration, { ...makeCuration(), checks: { passed: false, results: [] } }).ok).toBe(
      false,
    );
    expect(check(schemas.validateCuration, { ...makeCuration(), rationale: "why" }).ok).toBe(false);
  });
});

describe("report intake", () => {
  const ok = { page_id: "test-page", text: "The CPI figure looks wrong." };
  it("accepts a minimal report and one with an e-mail", () => {
    expect(check(schemas.validateReportIntake, ok).ok).toBe(true);
    expect(check(schemas.validateReportIntake, { ...ok, email: "reader@example.org" }).ok).toBe(true);
  });
  it.each([
    { ...ok, text: "short" },
    { ...ok, text: "x".repeat(2001) },
    { ...ok, email: "not an email" },
    { ...ok, email: "a@b" },
    { ...ok, ip: "1.2.3.4" },
  ])("refuses %j", (body) => {
    expect(check(schemas.validateReportIntake, body).ok).toBe(false);
  });
});
