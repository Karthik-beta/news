// Parses every JSON, JSONC, YAML and TOML config file in the repo, so a stray comma or bad indent fails fast with
// the file name. Run from the repo root: `bun scripts/validate-config.ts`. It checks syntax only; schemas are the
// job of the tools that read each file (tsc, wrangler, Biome, actionlint).
//   .json                       strict JSON, except tsconfig*.json and biome.json, which allow comments
//   .jsonc                      JSON with comments and trailing commas (wrangler.jsonc)
//   .yml / .yaml                YAML (workflows, issue forms, Compose, collector, CodeRabbit)
//   .toml                       TOML (Cargo, cargo-deny, rust-toolchain, bunfig)
import { readFileSync } from "node:fs";
import { basename, join } from "node:path";
import { root } from "./lib";

export interface Problem {
  file: string;
  message: string;
}

const IGNORED =
  /(^|\/)(node_modules|\.git|target|dist|\.astro|\.wrangler|test-results|playwright-report|__screenshots__)\//;
// Other agents' worktrees sit inside the repo folder on the laptop; they are not part of this checkout.
const WORKTREES = /^\.claude\/worktrees\//;

export function isLenientJson(file: string): boolean {
  const name = basename(file);
  return name.endsWith(".jsonc") || /^tsconfig.*\.json$/.test(name) || name === "biome.json";
}

/** Parse `text` the way its extension says. Returns an error message, or undefined when it parses. */
export function parseProblem(file: string, text: string): string | undefined {
  try {
    if (file.endsWith(".json") || file.endsWith(".jsonc")) {
      if (isLenientJson(file)) Bun.JSONC.parse(text);
      else JSON.parse(text);
    } else if (file.endsWith(".yml") || file.endsWith(".yaml")) {
      Bun.YAML.parse(text);
    } else if (file.endsWith(".toml")) {
      Bun.TOML.parse(text);
    }
    return undefined;
  } catch (e) {
    return e instanceof Error ? e.message : String(e);
  }
}

/** Every config file under `repoRoot` (relative paths, sorted). */
export function configFiles(repoRoot: string): string[] {
  const glob = new Bun.Glob("**/*.{json,jsonc,yml,yaml,toml}");
  const found: string[] = [];
  for (const file of glob.scanSync({ cwd: repoRoot, dot: true })) {
    const rel = file.replaceAll("\\", "/");
    if (IGNORED.test(rel) || WORKTREES.test(rel)) continue;
    found.push(rel);
  }
  return found.sort();
}

export function validateConfigs(repoRoot: string): { problems: Problem[]; checked: number } {
  const files = configFiles(repoRoot);
  const problems: Problem[] = [];
  for (const file of files) {
    const message = parseProblem(file, readFileSync(join(repoRoot, file), "utf8"));
    if (message) problems.push({ file, message: message.split("\n")[0] as string });
  }
  return { problems, checked: files.length };
}

if (import.meta.main) {
  const { problems, checked } = validateConfigs(root);
  if (checked === 0) {
    console.error("No config files found: is this the repo root?");
    process.exit(1);
  }
  if (problems.length > 0) {
    for (const p of problems) console.error(`${p.file}: ${p.message}`);
    console.error(`\n${problems.length} of ${checked} config file(s) do not parse.`);
    process.exit(1);
  }
  console.log(`OK: ${checked} config files parse.`);
}
