// Lint test: fails if any Worker imports a model SDK or binds Workers AI (TD principle 3, ADR 0006).
import { readdirSync, readFileSync } from "node:fs";
import { join, relative } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { scanFile } from "./ai-guard";

const WORKERS = fileURLToPath(new URL("../../", import.meta.url));
// This guard and its test name the forbidden packages on purpose.
const SELF = new Set(["shared/src/ai-guard.ts", "shared/src/ai-guard.test.ts"]);
const SKIP_DIRS = new Set(["node_modules", ".wrangler", "dist", "generated"]);

function filesUnder(dir: string): string[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((e) => {
    if (e.isDirectory()) return SKIP_DIRS.has(e.name) ? [] : filesUnder(join(dir, e.name));
    return /\.(?:[cm]?[jt]sx?|json|jsonc|toml)$/.test(e.name) ? [join(dir, e.name)] : [];
  });
}

describe("no AI in the Workers", () => {
  it("no Worker source, package.json or wrangler config uses a model SDK or an AI binding", () => {
    const files = filesUnder(WORKERS).filter((f) => !SELF.has(relative(WORKERS, f).replaceAll("\\", "/")));
    expect(files.length).toBeGreaterThan(20);
    const findings = files.flatMap((f) => scanFile(relative(WORKERS, f), readFileSync(f, "utf8")));
    expect(findings).toEqual([]);
  });
});

describe("the scanner catches", () => {
  it.each([
    ['import Anthropic from "@anthropic-ai/sdk";', "@anthropic-ai/sdk"],
    ["import OpenAI from 'openai'", "openai"],
    ['const { generateText } = await import("ai");', "ai"],
    ['import { openai } from "@ai-sdk/openai";', "@ai-sdk/openai"],
    ['const g = require("@google/genai")', "@google/genai"],
    ['export * from "workers-ai-provider";', "workers-ai-provider"],
    ['import "@aws-sdk/client-bedrock-runtime";', "@aws-sdk/client-bedrock-runtime"],
  ])("%s", (code, detail) => {
    expect(scanFile("src/x.ts", code)).toEqual([{ file: "src/x.ts", line: 1, rule: "model-import", detail }]);
  });

  it("an AI binding used in code or declared in wrangler.jsonc", () => {
    expect(scanFile("src/x.ts", "const r = await env.AI.run(model, input);")[0]?.rule).toBe("ai-binding");
    expect(scanFile("wrangler.jsonc", '{\n  "ai": { "binding": "AI" }\n}')[0]).toMatchObject({
      rule: "ai-binding",
      line: 2,
    });
  });

  it.each([
    ['[ai]\nbinding = "AI"\n', 1],
    ['name = "x"\n\n[env.production.ai]\nbinding = "AI"\n', 3],
    ['name = "x"\n[ai_search]\nbinding = "S"\n', 2],
    ['name = "x"\nai = { binding = "AI" }\n', 2],
  ])("an AI binding in wrangler.toml: %j", (toml, line) => {
    expect(scanFile("wrangler.toml", toml)[0]).toMatchObject({ rule: "ai-binding", line });
  });

  it("a model SDK in package.json", () => {
    const pkg = JSON.stringify(
      { dependencies: { aws4fetch: "1" }, devDependencies: { "@anthropic-ai/sdk": "1" } },
      null,
      2,
    );
    expect(scanFile("package.json", pkg)).toEqual([
      { file: "package.json", line: 6, rule: "model-dependency", detail: "@anthropic-ai/sdk" },
    ]);
  });

  it("but not look-alikes or comments in wrangler.jsonc", () => {
    expect(scanFile("src/x.ts", 'import { x } from "aim"; import y from "./ai"; import "ai-guard-not";')).toEqual([]);
    expect(scanFile("wrangler.jsonc", '{\n  // never add "ai": bindings\n  "name": "x"\n}')).toEqual([]);
    expect(scanFile("wrangler.toml", '# never add [ai]\nname = "ai-free"\n[vars]\nMODE = "x"\n')).toEqual([]);
  });
});
