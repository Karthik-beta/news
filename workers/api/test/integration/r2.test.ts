// R2 inside workerd (Miniflare's local R2): the page-bundle layout the Technical Design describes, plus edge cases.
import { env } from "cloudflare:workers";
import { afterAll, describe, expect, it } from "vitest";

const bucket = () => env.PAGES;
// Every run uses its own key prefix, and removes what it wrote, so reruns and parallel runs never see each other.
const prefix = `test-${crypto.randomUUID()}/`;
const written = new Set<string>();
const put = async (key: string, value: string | ArrayBuffer | Uint8Array, options?: R2PutOptions) => {
  written.add(prefix + key);
  return bucket().put(prefix + key, value, options);
};
const text = async (key: string) => (await bucket().get(prefix + key))?.text();

afterAll(async () => {
  await bucket().delete([...written]);
  expect((await bucket().list({ prefix })).objects).toEqual([]);
});

describe("R2 objects", () => {
  it("puts and gets text, with size and etag", async () => {
    const put1 = await put("pages/p1/index.html", "<h1>Hello</h1>");
    expect(put1?.size).toBe(14);
    const got = await bucket().get(`${prefix}pages/p1/index.html`);
    expect(got?.etag).toBe(put1?.etag);
    expect(got?.size).toBe(14);
    expect(await got?.text()).toBe("<h1>Hello</h1>");
  });

  it("round-trips every byte value unchanged", async () => {
    const bytes = Uint8Array.from({ length: 256 }, (_, i) => i);
    await put("bin/all-bytes.bin", bytes);
    const back = new Uint8Array(
      (await (await bucket().get(`${prefix}bin/all-bytes.bin`))?.arrayBuffer()) ?? new ArrayBuffer(0),
    );
    expect(Array.from(back)).toEqual(Array.from(bytes));
  });

  it("round-trips an empty object and a 1 MiB object", async () => {
    await put("size/empty", "");
    expect((await bucket().head(`${prefix}size/empty`))?.size).toBe(0);
    expect(await text("size/empty")).toBe("");

    const big = new Uint8Array(1024 * 1024).fill(97);
    await put("size/big", big);
    expect((await bucket().head(`${prefix}size/big`))?.size).toBe(1024 * 1024);
  });

  it("keeps content type, cache headers and custom metadata", async () => {
    await put("pages/p1/meta.json", "{}", {
      httpMetadata: { contentType: "application/json", cacheControl: "public, max-age=3600" },
      customMetadata: { version: "3", renderer: "1.0.0" },
    });
    const head = await bucket().head(`${prefix}pages/p1/meta.json`);
    expect(head?.httpMetadata?.contentType).toBe("application/json");
    expect(head?.httpMetadata?.cacheControl).toBe("public, max-age=3600");
    expect(head?.customMetadata).toEqual({ version: "3", renderer: "1.0.0" });
  });

  it("returns null for a missing key from get and head", async () => {
    expect(await bucket().get(`${prefix}nope`)).toBeNull();
    expect(await bucket().head(`${prefix}nope`)).toBeNull();
  });

  it("handles unicode and URL-special characters in keys", async () => {
    const key = "news/ünï/a b#frag?x=1&y.json";
    await put(key, "ok");
    expect(await text(key)).toBe("ok");
    expect((await bucket().list({ prefix: `${prefix}news/` })).objects.map((o) => o.key)).toEqual([prefix + key]);
  });
});

describe("R2 revisions", () => {
  it("overwriting a key replaces the content and changes the etag", async () => {
    const first = await put("pages/p2/index.html", "v1");
    const second = await put("pages/p2/index.html", "version two");
    expect(second?.etag).not.toBe(first?.etag);
    expect(await text("pages/p2/index.html")).toBe("version two");
  });

  it("putting identical content twice is idempotent", async () => {
    const a = await put("pages/p3/data.json", '{"a":1}');
    const b = await put("pages/p3/data.json", '{"a":1}');
    expect(b?.etag).toBe(a?.etag);
    expect((await bucket().list({ prefix: `${prefix}pages/p3/` })).objects).toHaveLength(1);
  });

  it("a conditional put with the wrong etag does not overwrite", async () => {
    await put("pages/p4/index.html", "original");
    const res = await put("pages/p4/index.html", "intruder", { onlyIf: { etagMatches: "not-the-etag" } });
    expect(res).toBeNull();
    expect(await text("pages/p4/index.html")).toBe("original");
  });

  it("a conditional put with the current etag does overwrite", async () => {
    const current = await put("pages/p5/index.html", "original");
    const res = await put("pages/p5/index.html", "updated", { onlyIf: { etagMatches: current?.etag ?? "" } });
    expect(res).not.toBeNull();
    expect(await text("pages/p5/index.html")).toBe("updated");
  });
});

describe("R2 delete, list and range", () => {
  it("delete removes the object, and deleting a missing key does not throw", async () => {
    await put("pages/gone/index.html", "x");
    await bucket().delete(`${prefix}pages/gone/index.html`);
    expect(await bucket().head(`${prefix}pages/gone/index.html`)).toBeNull();
    await expect(bucket().delete(`${prefix}pages/gone/index.html`)).resolves.toBeUndefined();
  });

  it("lists by prefix, in key order, and folds folders with a delimiter", async () => {
    for (const key of ["pages/a/index.html", "pages/a/data.json", "pages/b/index.html", "assets/logo.svg"])
      await put(`list/${key}`, "x");

    const a = await bucket().list({ prefix: `${prefix}list/pages/a/` });
    expect(a.objects.map((o) => o.key.slice(prefix.length))).toEqual([
      "list/pages/a/data.json",
      "list/pages/a/index.html",
    ]);

    const folders = await bucket().list({ prefix: `${prefix}list/pages/`, delimiter: "/" });
    expect(folders.objects).toEqual([]);
    expect(folders.delimitedPrefixes.map((p) => p.slice(prefix.length))).toEqual(["list/pages/a/", "list/pages/b/"]);
  });

  it("paginates a listing with a cursor without repeats or gaps", async () => {
    const keys = Array.from({ length: 7 }, (_, i) => `page/${String(i).padStart(2, "0")}`);
    for (const key of keys) await put(key, "x");
    const seen: string[] = [];
    let cursor: string | undefined;
    do {
      const res = await bucket().list({ prefix: `${prefix}page/`, limit: 3, cursor });
      seen.push(...res.objects.map((o) => o.key.slice(prefix.length)));
      cursor = res.truncated ? res.cursor : undefined;
    } while (cursor);
    expect(seen).toEqual(keys);
  });

  it("returns an empty listing for a prefix with nothing under it", async () => {
    const res = await bucket().list({ prefix: `${prefix}empty/` });
    expect(res.objects).toEqual([]);
    expect(res.truncated).toBe(false);
  });

  it("reads a byte range", async () => {
    await put("range/digits", "0123456789");
    const mid = await bucket().get(`${prefix}range/digits`, { range: { offset: 2, length: 4 } });
    expect(await mid?.text()).toBe("2345");
    const tail = await bucket().get(`${prefix}range/digits`, { range: { suffix: 3 } });
    expect(await tail?.text()).toBe("789");
  });
});
