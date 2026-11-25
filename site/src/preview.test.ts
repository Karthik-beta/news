// Preview mode (ADR 0010, section 6): local only, fails closed, holds a draft curation in memory, marks every response.
import { sha256Hex } from "@news/worker-shared/crypto";
import { afterEach, describe, expect, it } from "vitest";
import { dataOf, FIXTURE_ITEMS, tableOf } from "../tests/support/site-data";
import { assertLocalPreview, decorate, handleDraft, withDraft } from "./preview";

const LOCAL = { NEWS_ENV: "local" };
/** The fixture curation with another lead: a draft is a curation.v1 body, without the storage fields. */
const draftBody = (() => {
  const live = (FIXTURE_ITEMS.normal ?? []).find((i) => i.PK === "HOME#CURATION") as unknown as Record<string, unknown>;
  const { schema, lang, created_at, approved_at, curator_model, slots, checks } = live as unknown as {
    schema: string;
    lang: string;
    created_at: string;
    approved_at: string;
    curator_model: string;
    slots: object;
    checks: object;
  };
  return JSON.stringify({
    schema,
    lang,
    created_at,
    approved_at,
    curator_model,
    checks,
    slots: { ...slots, lead: { page_id: "japan-wages" } },
  });
})();
const put = (body: string, url = "http://127.0.0.1:4321/__preview/curation", env = LOCAL) =>
  handleDraft(new Request(url, { method: "PUT", body, headers: { "content-type": "application/json" } }), env);
const clear = () => handleDraft(new Request("http://127.0.0.1:4321/__preview/curation", { method: "DELETE" }), LOCAL);

afterEach(clear);

describe("fails closed", () => {
  it("refuses to start outside NEWS_ENV=local", () => {
    for (const env of [{ NEWS_ENV: "production" }, {}, { NEWS_ENV: "test" }])
      expect(() => assertLocalPreview(env)).toThrow(/refused/);
    expect(() => assertLocalPreview(LOCAL)).not.toThrow();
  });

  it("the handlers answer 404 in production and to a host that is not loopback", async () => {
    expect((await put(draftBody, undefined, { NEWS_ENV: "production" })).status).toBe(404);
    expect((await put(draftBody, "https://news.karthik.nz/__preview/curation")).status).toBe(404);
    expect((await put(draftBody, "http://192.168.1.5:4321/__preview/curation")).status).toBe(404);
    expect((await put(draftBody, "http://[::1]:4321/__preview/curation")).status).toBe(204);
  });
});

describe("the draft", () => {
  it("is validated against curation.v1 and refused when it is not", async () => {
    expect((await put("not json")).status).toBe(400);
    const res = await put(JSON.stringify({ schema: "curation.v1" }));
    expect(res.status).toBe(400);
    expect(await res.text()).toContain("not curation.v1");
    expect((await put("x".repeat(300 * 1024))).status).toBe(413);
  });

  it("replaces the live curation while held, and DELETE brings the live one back", async () => {
    const data = withDraft(dataOf(tableOf(FIXTURE_ITEMS.normal)));
    const live = (await data.curation())?.slots.lead.page_id;
    expect(live).toBe("apple-fy2024-results");
    await put(draftBody);
    expect((await data.curation())?.slots.lead.page_id).toBe("japan-wages");
    await clear();
    expect((await data.curation())?.slots.lead.page_id).toBe(live);
  });
});

describe("every response", () => {
  it("is no-store and noindex, and carries the sha256 of the draft's bytes only while one is held", async () => {
    const page = () => decorate(new Response("<p>home</p>", { headers: { "cache-control": "public, s-maxage=300" } }));
    expect(page().headers.get("cache-control")).toBe("no-store");
    expect(page().headers.get("x-robots-tag")).toBe("noindex");
    expect(page().headers.has("x-news-preview")).toBe(false);
    await put(draftBody);
    expect(page().headers.get("x-news-preview")).toBe(await sha256Hex(draftBody));
    await clear();
    expect(page().headers.has("x-news-preview")).toBe(false);
  });
});
