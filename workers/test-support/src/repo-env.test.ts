import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { parseDotenv, repoEnv } from "./repo-env";

describe("parseDotenv", () => {
  it("reads KEY=value lines, skips comments, strips one pair of matching quotes, first value wins", () => {
    const text = "# c\nA=1\nB=\"two\"\nC='three'\nD=\"mis'\nA=again\n\n  E = spaced \r\n";
    expect(parseDotenv(text)).toEqual({ A: "1", B: "two", C: "three", D: "\"mis'", E: "spaced" });
  });
});

describe("repoEnv", () => {
  const withFile = (text: string, fn: (path: string) => void) => {
    const dir = mkdtempSync(join(tmpdir(), "repo-env-"));
    const path = join(dir, ".env");
    writeFileSync(path, text);
    try {
      fn(path);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  };

  it("uses the root .env endpoint when the process has none or an empty one", () => {
    withFile("NEWS_DYNAMODB_ENDPOINT=http://127.0.0.1:8123\n", (path) => {
      expect(repoEnv({}, path).NEWS_DYNAMODB_ENDPOINT).toBe("http://127.0.0.1:8123");
      expect(repoEnv({ NEWS_DYNAMODB_ENDPOINT: "" }, path).NEWS_DYNAMODB_ENDPOINT).toBe("http://127.0.0.1:8123");
    });
  });

  it("lets an exported variable win over the file", () => {
    withFile("NEWS_DYNAMODB_ENDPOINT=http://127.0.0.1:8123\n", (path) => {
      expect(repoEnv({ NEWS_DYNAMODB_ENDPOINT: "http://127.0.0.1:9000" }, path).NEWS_DYNAMODB_ENDPOINT).toBe(
        "http://127.0.0.1:9000",
      );
    });
  });

  it("works without a root .env", () => {
    expect(repoEnv({ X: "1" }, join(tmpdir(), "no-such-dir-repo-env", ".env"))).toEqual({ X: "1" });
  });
});
