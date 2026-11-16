// "AI lives only on the laptop" (TD, design principle 3; ADR 0006): no Worker may import a model SDK or bind
// Workers AI. ai-guard.test.ts scans every Worker's sources, package.json and wrangler.jsonc with these rules.

/** Packages that call a model, matched as the whole name or as a scope/prefix followed by '/'. */
export const MODEL_PACKAGES = [
  "@anthropic-ai",
  "anthropic",
  "openai",
  "@openai",
  "@google/generative-ai",
  "@google/genai",
  "@google-cloud/vertexai",
  "ai",
  "@ai-sdk",
  "cohere-ai",
  "@mistralai",
  "ollama",
  "langchain",
  "@langchain",
  "llamaindex",
  "workers-ai-provider",
  "@cloudflare/ai",
  "replicate",
  "@huggingface/inference",
  "groq-sdk",
  "@aws-sdk/client-bedrock-runtime",
  "@aws-sdk/client-bedrock",
  "@aws-sdk/client-bedrock-agent-runtime",
] as const;

export interface Finding {
  file: string;
  line: number;
  rule: "model-import" | "model-dependency" | "ai-binding";
  detail: string;
}

export function isModelPackage(specifier: string): boolean {
  return MODEL_PACKAGES.some((p) => specifier === p || specifier.startsWith(`${p}/`));
}

const IMPORT = /(?:\bfrom\s*|\bimport\s*\(\s*|\brequire\s*\(\s*|\bimport\s+)["']([^"'\n]+)["']/g;
// Workers AI and AI Gateway bindings in a wrangler config, and code that reaches for an AI binding on env.
const BINDING_CONFIG = /"(ai|ai_search)"\s*:/;
// TOML: a table `[ai]` / `[ai_search]`, also per environment (`[env.production.ai]`), an inline `ai = { … }` or a
// dotted key `ai.binding = …`. A key may be bare, "quoted" or 'literal' (`["ai"]` is the same table as `[ai]`).
const TOML_SEGMENT = String.raw`(?:[A-Za-z0-9_-]+|"[^"\n]*"|'[^'\n]*')`;
const TOML_AI_KEY = `(?:"(ai|ai_search)"|'(ai|ai_search)'|(ai|ai_search))`;
const TOML_ENV = String.raw`(?:(?:env|"env"|'env')[ \t]*\.[ \t]*${TOML_SEGMENT}[ \t]*\.[ \t]*)?`;
const BINDING_TOML = new RegExp(
  String.raw`^[ \t]*(?:\[[ \t]*${TOML_ENV}${TOML_AI_KEY}[ \t]*[\].]|${TOML_ENV}${TOML_AI_KEY}[ \t]*[=.])`,
  "m",
);
const BINDING_CODE = /\benv\s*\.\s*AI\b|\benv\s*\[\s*["']AI["']\s*\]/;

const lineOf = (text: string, index: number) => text.slice(0, index).split("\n").length;

/** Scans one file's text. `file` decides which rules apply (.ts/.js sources, package.json, wrangler.json[c]). */
export function scanFile(file: string, text: string): Finding[] {
  const findings: Finding[] = [];
  const name = file.split(/[\\/]/).pop() ?? file;
  if (name === "package.json") {
    const pkg = JSON.parse(text) as Record<string, Record<string, string> | undefined>;
    for (const field of ["dependencies", "devDependencies", "peerDependencies", "optionalDependencies"]) {
      for (const dep of Object.keys(pkg[field] ?? {})) {
        if (isModelPackage(dep))
          findings.push({ file, line: lineOf(text, text.indexOf(`"${dep}"`)), rule: "model-dependency", detail: dep });
      }
    }
    return findings;
  }
  if (/^wrangler\.(jsonc?|toml)$/.test(name)) {
    // Comments may mention the rule; only live configuration counts.
    const toml = name.endsWith(".toml");
    // [ \t]*, not \s*: a match must not swallow the line breaks before it, or the reported line numbers shift.
    const live = text.replace(toml ? /^[ \t]*#.*$/gm : /^[ \t]*\/\/.*$/gm, "");
    const m = (toml ? BINDING_TOML : BINDING_CONFIG).exec(live);
    if (m)
      findings.push({
        file,
        line: lineOf(live, m.index),
        rule: "ai-binding",
        detail: `"${m.slice(1).find((g) => g !== undefined)}" binding`,
      });
    return findings;
  }
  if (/\.(?:[cm]?[jt]sx?)$/.test(name)) {
    for (const m of text.matchAll(IMPORT)) {
      const spec = m[1] as string;
      if (isModelPackage(spec))
        findings.push({ file, line: lineOf(text, m.index ?? 0), rule: "model-import", detail: spec });
    }
    const b = BINDING_CODE.exec(text);
    if (b) findings.push({ file, line: lineOf(text, b.index), rule: "ai-binding", detail: b[0] });
  }
  return findings;
}
