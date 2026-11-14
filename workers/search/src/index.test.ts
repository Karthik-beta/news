import { describe, expect, it } from "vitest";
import worker from "./index";

describe("news-search", () => {
  it("answers /health", async () => {
    const res = await worker.fetch(new Request("http://localhost/health"));
    expect(res.status).toBe(200);
    expect(await res.json()).toEqual({ worker: "news-search", status: "ok" });
  });

  it("returns 501 for everything else until Phase 6", async () => {
    const res = await worker.fetch(new Request("http://localhost/anything"));
    expect(res.status).toBe(501);
  });
});
