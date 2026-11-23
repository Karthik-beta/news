// Unit tests (inside workerd): the robots meta tag in R2's index.html follows index_status (#56, ADR 0004).

import { type Bundle, makeBundle, makeMeta } from "@news/worker-shared/testing/fixtures";
import { describe, expect, it } from "vitest";
import { findRobotsTag, ROBOTS_TAGS, withRobots } from "./robots";
import { harness } from "./testing";

type H = ReturnType<typeof harness>;

const HTML_KEY = "2026/11/test-page/index.html";
const NOINDEX = ROBOTS_TAGS.noindex;
const INDEX = ROBOTS_TAGS.index;
const errorOf = async (r: Response) =>
  ((await r.clone().json()) as { error: { code: string; details?: { code: string }[] } }).error;
const latest = (h: H) => h.table.dump().find((i) => i.PK === "PAGE#test-page" && i.SK === "LATEST");
/** The robots tag R2 serves now, and LATEST's index_status: the two must agree after every finished request. */
const state = (h: H) => {
  const html = h.bucket.text(HTML_KEY) ?? "";
  const tag = findRobotsTag(html);
  return { r2: "code" in tag ? tag.code : html.slice(tag.start, tag.end), latest: latest(h)?.index_status };
};
const put = (h: H, bundle: Bundle, key?: string) =>
  h.call("PUT", "/v1/pages/test-page", { body: bundle, ...(key ? { key } : {}) });
const setIndex = (h: H, index_status: "index" | "noindex", key?: string) =>
  h.call("POST", "/v1/pages/test-page/index", { body: { index_status }, ...(key ? { key } : {}) });

async function published(h: H) {
  expect((await put(h, await makeBundle())).status).toBe(200);
  expect(state(h)).toEqual({ r2: NOINDEX, latest: "noindex" });
}

describe("findRobotsTag", () => {
  const doc = (head: string, body = "<p>Robots and bots at work.</p>") =>
    `<!doctype html><html><head><meta charset="utf-8">${head}</head><body>${body}</body></html>`;
  const code = (html: string) => {
    const r = findRobotsTag(html);
    return "code" in r ? r.code : "ok";
  };

  it("finds the one tag in either form, and the rewrite changes only that tag", () => {
    for (const tag of [NOINDEX, INDEX]) {
      const html = doc(`<meta name="description" content="Robots > bots">${tag}`);
      expect(code(html)).toBe("ok");
      expect(withRobots(html, "index")).toBe(html.replace(tag, INDEX));
      expect(withRobots(html, "noindex")).toBe(html.replace(tag, NOINDEX));
    }
  });

  it("ignores the word in text, other meta tags and <metadata>", () => {
    const html = doc(NOINDEX, `<svg><metadata>name="robots"</metadata></svg><p>meta name="robots"</p>`);
    expect(code(html)).toBe("ok");
  });

  it("refuses no tag, two tags, and any other robots directive", () => {
    expect(code(doc(""))).toBe("bundle.robots_missing");
    expect(code(doc(NOINDEX + INDEX))).toBe("bundle.robots_duplicate");
    expect(code(doc(`${INDEX}<meta name="googlebot" content="noindex">`))).toBe("bundle.robots_duplicate");
    expect(code(doc(`${NOINDEX}<meta http-equiv="X-Robots-Tag" content="all">`))).toBe("bundle.robots_duplicate");
    // Counted wherever it is: a tag in a comment or a script is refused, never guessed to be inert.
    expect(code(doc(NOINDEX, `<!-- ${INDEX} -->`))).toBe("bundle.robots_duplicate");
    expect(code(doc(NOINDEX, `<script>"<meta name='x'>"</script>`))).toBe("bundle.robots_invalid");
    // A '>' inside a quoted value does not end the tag.
    expect(code(doc(`${NOINDEX}<meta content="a>b" name="robots">`))).toBe("bundle.robots_duplicate");
  });

  it("refuses a robots tag in any form but the renderer's two", () => {
    for (const head of [
      '<META NAME="ROBOTS" CONTENT="noindex">',
      '<meta name="robots" content="index">',
      '<meta name="robots" content="noindex" />',
      '<meta content="noindex" name="robots">',
      '<meta  name="robots" content="noindex">',
    ])
      expect(code(doc(head)), head).toBe("bundle.robots_invalid");
  });

  it("refuses a meta tag it cannot read for certain", () => {
    for (const head of [
      `${NOINDEX}<meta name='robots' content='index'>`,
      `${NOINDEX}<meta name=robots content=index>`,
      `${NOINDEX}<meta/name="robots" content="index">`,
      `${NOINDEX}<meta name="robots"content="index">`,
      `${NOINDEX}<meta name="&#114;obots" content="index">`,
      `${NOINDEX}<meta name="robots`,
    ])
      expect(code(doc(head)), head).toBe("bundle.robots_invalid");
  });

  it("withRobots refuses a file without exactly one tag (invalid_bundle)", () => {
    expect(() => withRobots(doc(""), "index")).toThrow(expect.objectContaining({ code: "invalid_bundle" }));
  });
});

describe("publish refuses a bundle without exactly one robots tag", () => {
  for (const [what, html, detail] of [
    ["no tag", "<!doctype html><title>x</title>", "bundle.robots_missing"],
    ["two tags", `<!doctype html>${NOINDEX}${INDEX}<title>x</title>`, "bundle.robots_duplicate"],
  ] as const)
    it(`${what} → 400 invalid_bundle (${detail}), nothing written`, async () => {
      const h = harness();
      const res = await put(h, await makeBundle(makeMeta(), { "index.html": html }));
      const error = await errorOf(res);
      expect([res.status, error.code]).toEqual([400, "invalid_bundle"]);
      expect(error.details?.map((d) => d.code)).toEqual([detail]);
      expect(h.writeCount()).toBe(0);
    });
});

describe("the R2 robots tag follows index_status", () => {
  it("promote, then withdraw from the index: the tag equals LATEST after each, and the file keeps its stamp", async () => {
    const h = harness();
    await published(h);
    const before = h.bucket.text(HTML_KEY) as string;
    const stamp = h.bucket.objects.get(HTML_KEY)?.customMetadata;
    expect((await setIndex(h, "index")).status).toBe(200);
    expect(state(h)).toEqual({ r2: INDEX, latest: "index" });
    expect(h.bucket.text(HTML_KEY)).toBe(before.replace(NOINDEX, INDEX));
    expect(h.bucket.objects.get(HTML_KEY)?.customMetadata).toEqual(stamp);
    expect((await setIndex(h, "noindex")).status).toBe(200);
    expect(state(h)).toEqual({ r2: NOINDEX, latest: "noindex" });
    expect(h.bucket.text(HTML_KEY)).toBe(before);
  });

  it("a bundle sent as index is published noindex when the live page is noindex (the live status wins)", async () => {
    const h = harness();
    await published(h);
    const v2 = await makeBundle(makeMeta({ version: 2, index_status: "index" }));
    expect((await put(h, v2)).status).toBe(200);
    expect(state(h)).toEqual({ r2: NOINDEX, latest: "noindex" });
  });

  it("promotion, crash after the commit and before the R2 write: R2 stays noindex; the retry writes index", async () => {
    const h = harness();
    await published(h);
    const bucketPut = h.bucket.put.bind(h.bucket);
    h.bucket.put = async () => {
      throw new Error("R2 unavailable (test)");
    };
    expect((await setIndex(h, "index", "promote-0001")).status).toBe(500);
    // Between the two writes the served page is never more open than the record.
    expect(state(h)).toEqual({ r2: NOINDEX, latest: "index" });
    h.bucket.put = bucketPut;
    expect((await setIndex(h, "index", "promote-0001")).status).toBe(200);
    expect(state(h)).toEqual({ r2: INDEX, latest: "index" });
  });

  it("withdrawal from the index, crash after the R2 write and before the commit: R2 is noindex; the retry commits", async () => {
    const h = harness();
    await published(h);
    expect((await setIndex(h, "index")).status).toBe(200);
    const transact = h.table.transact.bind(h.table);
    h.table.transact = async () => {
      throw new Error("DynamoDB unavailable (test)");
    };
    expect((await setIndex(h, "noindex", "demote-0001")).status).toBe(500);
    expect(state(h)).toEqual({ r2: NOINDEX, latest: "index" });
    h.table.transact = transact;
    expect((await setIndex(h, "noindex", "demote-0001")).status).toBe(200);
    expect(state(h)).toEqual({ r2: NOINDEX, latest: "noindex" });
  });

  it("a retried older publish never undoes a newer promotion", async () => {
    const h = harness();
    await published(h);
    // v2 commits, then its purge fails: the laptop will retry it with the same key.
    const v2 = await makeBundle(makeMeta({ version: 2, title: "Version two" }));
    h.purger.failNext = true;
    expect((await put(h, v2, "publish-v2-0001")).status).toBe(502);
    expect((await setIndex(h, "index")).status).toBe(200);
    expect((await put(h, v2, "publish-v2-0001")).status).toBe(200);
    expect(state(h)).toEqual({ r2: INDEX, latest: "index" });
    // The same bundle sent again under a new key, and v1 sent again, change nothing either.
    expect((await put(h, v2)).status).toBe(200);
    expect((await put(h, await makeBundle())).status).toBe(409);
    expect(state(h)).toEqual({ r2: INDEX, latest: "index" });
  });

  it("a stored index.html without a tag the api can rewrite → 400 invalid_bundle, nothing changed", async () => {
    const h = harness();
    await published(h);
    const stamp = h.bucket.objects.get(HTML_KEY)?.customMetadata;
    await h.bucket.put(HTML_KEY, "<!doctype html><title>old</title>", { customMetadata: stamp });
    const writes = h.bucket.writes.length;
    const res = await setIndex(h, "index");
    expect([res.status, (await errorOf(res)).code]).toEqual([400, "invalid_bundle"]);
    expect(latest(h)?.index_status).toBe("noindex");
    expect(h.bucket.writes.length).toBe(writes);
  });
});
