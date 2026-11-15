import { describe, expect, test } from "bun:test";
import { DEFAULT_PORT, resolvePort } from "./port";

describe("resolvePort", () => {
  test("unset or empty uses the default 4400", () => {
    expect(DEFAULT_PORT).toBe(4400);
    expect(resolvePort(undefined)).toEqual({ port: 4400 });
    expect(resolvePort("")).toEqual({ port: 4400 });
    expect(resolvePort("   ")).toEqual({ port: 4400 });
  });

  test("accepts whole numbers from 1 to 65535", () => {
    for (const [raw, port] of [
      ["1", 1],
      ["4400", 4400],
      ["65535", 65535],
      [" 8080 ", 8080],
    ] as const) {
      expect(resolvePort(raw)).toEqual({ port });
    }
  });

  test("rejects anything else with a one-line error naming the variable and the value", () => {
    for (const bad of ["abc", "70000", "65536", "0", "-1", "1.5", "1e3", "0x10", "80abc", "+80", "Infinity"]) {
      const r = resolvePort(bad);
      expect("error" in r, bad).toBe(true);
      if (!("error" in r)) continue;
      expect(r.error).toContain("NEWS_DASHBOARD_PORT");
      expect(r.error).toContain(JSON.stringify(bad));
      expect(r.error).not.toContain("\n");
    }
  });
});
