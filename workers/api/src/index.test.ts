// Unit tests (inside workerd): routing, authentication and request validation. Every error body must match the
// contract's error schema, with one of its codes.
import { exports } from "cloudflare:workers";
import { makeBundle } from "@news/worker-shared/testing/fixtures";
import { schemas } from "@news/worker-shared/validate";
import { describe, expect, it } from "vitest";
import { countingLimiter, harness, TEST_KEY } from "./testing";

async function expectError(res: Response, status: number, code: string) {
  expect(res.status).toBe(status);
  expect(res.headers.get("content-type")).toContain("application/json");
  const body = await res.json();
  expect(schemas.validateErrorBody(body), JSON.stringify(schemas.validateErrorBody.errors)).toBe(true);
  expect((body as { error: { code: string } }).error.code).toBe(code);
  return body as { error: { code: string; message: string; details?: { code: string; path?: string }[] } };
}

describe("the deployed entry point", () => {
  it("answers /health without any binding", async () => {
    const res = await exports.default.fetch(new Request("http://localhost/health"));
    expect(res.status).toBe(200);
    expect(await res.json()).toEqual({ worker: "news-api", status: "ok" });
  });

  it("fails closed without a configured write key", async () => {
    const res = await exports.default.fetch(
      new Request("http://localhost/v1/home/curation", { method: "PUT", headers: { authorization: "Bearer " } }),
    );
    await expectError(res, 401, "unauthorized");
  });
});

describe("error codes", () => {
  it("are exactly the contract's list", async () => {
    const { STATUS } = await import("./errors");
    const contract = (await import("../../../contracts/cloud/write-api.schema.json")).default as {
      $defs: { error: { properties: { error: { properties: { code: { enum: string[] } } } } } };
    };
    expect(Object.keys(STATUS).sort()).toEqual([...contract.$defs.error.properties.error.properties.code.enum].sort());
  });
});

describe("routing", () => {
  it("health: GET and HEAD only", async () => {
    const h = harness();
    expect((await h.call("GET", "/health")).status).toBe(200);
    expect((await h.call("HEAD", "/health")).status).toBe(200);
    await expectError(await h.call("POST", "/health"), 405, "method_not_allowed");
  });

  it.each(["/", "/v1", "/v1/pages", "/v1/pages/UPPER", "/v1/pages/a/b", "/v1/pages/x%2Fy", "/v2/pages/x", "/health/"])(
    "%s is not found",
    async (path) => {
      await expectError(await harness().call("PUT", path, { body: {} }), 404, "not_found");
    },
  );

  it("a known path with the wrong method is 405, but only after authentication", async () => {
    const h = harness();
    const res = await h.call("GET", "/v1/pages/abc");
    await expectError(res, 405, "method_not_allowed");
    expect(res.headers.get("allow")).toBe("PUT");
    await expectError(await h.call("GET", "/v1/pages/abc", { auth: null }), 401, "unauthorized");
  });
});

describe("authentication", () => {
  it.each([
    ["no header", null],
    ["empty bearer", "Bearer "],
    ["wrong key", "Bearer wrong-key-0123456789abcdef-0123456789"],
    ["key with a suffix", `Bearer ${TEST_KEY}x`],
    ["key prefix", `Bearer ${TEST_KEY.slice(0, -1)}`],
    ["wrong scheme", `Basic ${TEST_KEY}`],
    ["lower-case scheme", `bearer ${TEST_KEY}`],
  ])("refuses %s with 401 and writes nothing", async (_label, auth) => {
    const h = harness();
    const res = await h.call("PUT", "/v1/home/curation", { auth, body: {} });
    await expectError(res, 401, "unauthorized");
    expect(res.headers.get("www-authenticate")).toContain("Bearer");
    expect(h.writeCount()).toBe(0);
    expect(h.table.calls).toEqual([]);
  });

  it("fails closed when no key, or a short key, is configured", async () => {
    for (const writeKey of [undefined, "", "short-key"]) {
      const h = harness({ writeKey });
      await expectError(
        await h.call("PUT", "/v1/home/curation", { auth: `Bearer ${writeKey ?? ""}`, body: {} }),
        401,
        "unauthorized",
      );
      expect(h.logs.some((l) => l.event === "write_key_not_configured")).toBe(true);
    }
  });

  it("rate-limits failed attempts per client", async () => {
    const authFailure = countingLimiter(3);
    const h = harness({ limiters: { authFailure } });
    const attempt = () =>
      h.call("PUT", "/v1/home/curation", {
        auth: "Bearer nope",
        body: {},
        headers: { "cf-connecting-ip": "203.0.113.9" },
      });
    for (let i = 0; i < 3; i++) await expectError(await attempt(), 401, "unauthorized");
    const res = await attempt();
    await expectError(res, 429, "rate_limited");
    expect(res.headers.get("retry-after")).toBe("60");
    // A valid key is never limited.
    expect(
      (await h.call("PUT", "/v1/home/curation", { body: {}, headers: { "cf-connecting-ip": "203.0.113.9" } })).status,
    ).toBe(400);
  });

  it("never logs the key", async () => {
    const h = harness();
    await h.call("PUT", "/v1/home/curation", { body: { x: 1 } });
    await h.call("PUT", "/v1/home/curation", { auth: "Bearer guess-guess-guess-guess-guess-guess", body: {} });
    expect(JSON.stringify(h.logs)).not.toContain(TEST_KEY);
    expect(JSON.stringify(h.logs)).not.toContain("guess-guess");
  });
});

describe("request validation", () => {
  it.each([
    null,
    "text/plain",
    "application/x-www-form-urlencoded",
    "application/json; charset=latin1",
    "multipart/form-data",
  ])("content type %s → 400 unsupported_content_type", async (contentType) => {
    await expectError(
      await harness().call("PUT", "/v1/home/curation", { contentType, body: {} }),
      400,
      "unsupported_content_type",
    );
  });

  it("a missing or malformed Idempotency-Key → 400", async () => {
    const h = harness();
    await expectError(
      await h.call("PUT", "/v1/home/curation", { key: null, body: {} }),
      400,
      "idempotency_key_required",
    );
    for (const key of ["short", "has space here", "x".repeat(256), "semi;colon-key"]) {
      await expectError(await h.call("PUT", "/v1/home/curation", { key, body: {} }), 400, "idempotency_key_invalid");
    }
  });

  it("an oversize body → 413, by Content-Length and while streaming", async () => {
    const h = harness();
    const big = JSON.stringify({ pad: "x".repeat(70 * 1024) });
    await expectError(await h.call("PUT", "/v1/home/curation", { raw: big }), 413, "payload_too_large");
    const stream = new ReadableStream({
      start(c) {
        for (let i = 0; i < 80; i++) c.enqueue(new TextEncoder().encode("x".repeat(1024)));
        c.close();
      },
    });
    await expectError(await h.call("PUT", "/v1/home/curation", { raw: stream }), 413, "payload_too_large");
    const page = await h.call("PUT", "/v1/pages/abc", { raw: "x".repeat(5 * 1024 * 1024 + 1) });
    await expectError(page, 413, "payload_too_large");
    expect(h.writeCount()).toBe(0);
  });

  it("invalid JSON and invalid UTF-8 → 400 invalid_json", async () => {
    const h = harness();
    await expectError(await h.call("PUT", "/v1/home/curation", { raw: "{not json" }), 400, "invalid_json");
    await expectError(
      await h.call("PUT", "/v1/home/curation", { raw: new Uint8Array([0x7b, 0xff, 0x7d]) }),
      400,
      "invalid_json",
    );
  });

  it("a body that fails the schema → 400 with pointer-only details", async () => {
    const h = harness();
    const body = await expectError(
      await h.call("PUT", "/v1/pages/abc", { body: { manifest: {}, files: {} } }),
      400,
      "invalid_request",
    );
    expect(body.error.details?.length).toBeGreaterThan(0);
    expect(h.writeCount()).toBe(0);
  });

  it("invalid meta.json → 400 invalid_meta; inconsistent bundle → 400 invalid_bundle", async () => {
    const h = harness();
    const bundle = await makeBundle();
    const tampered = { ...bundle, files: { ...bundle.files, "index.html": "<p>changed</p>" } };
    const bad = await expectError(
      await h.call("PUT", "/v1/pages/test-page", { body: tampered }),
      400,
      "invalid_bundle",
    );
    expect(bad.error.details?.map((d) => d.code)).toContain("bundle.size_mismatch");

    const { makeMeta } = await import("@news/worker-shared/testing/fixtures");
    const noLang = { ...makeMeta(), lang: undefined };
    const withBadMeta = await makeBundle(makeMeta(), { "meta.json": JSON.stringify(noLang) });
    await expectError(await h.call("PUT", "/v1/pages/test-page", { body: withBadMeta }), 400, "invalid_meta");
    expect(h.writeCount()).toBe(0);
  });
});
