// Parses every JSON, JSONC, YAML and TOML config file in the repo, so a stray comma or bad indent fails fast with
// the file name. Run from the repo root: `bun scripts/validate-config.ts`. It checks syntax only; schemas are the
// job of the tools that read each file (tsc, wrangler, Biome, actionlint).
//   .json                       strict JSON, except tsconfig*.json and biome.json, which allow comments
//   .jsonc                      JSON with comments and trailing commas (wrangler.jsonc)
//   .yml / .yaml                YAML (workflows, issue forms, Compose, collector, CodeRabbit)
//   .toml                       TOML (Cargo, cargo-deny, rust-toolchain, bunfig)
import { readdirSync, readFileSync } from "node:fs";
import { basename, join } from "node:path";
import { DEFAULT_SITE_ORIGIN, regionViews } from "../workers/shared/src/urls";
import { root } from "./lib";

export interface Problem {
  file: string;
  message: string;
}

// Folders skipped wherever they appear. They are pruned during the walk and never opened: node_modules and other
// agents' worktrees hold tens of thousands of files, and scanning them before filtering took seconds.
const IGNORED = new Set([
  "node_modules",
  ".git",
  "target",
  "dist",
  ".astro",
  ".wrangler",
  "test-results",
  "playwright-report",
  "__screenshots__",
]);
// Other agents' worktrees sit inside the repo folder on the laptop; they are not part of this checkout.
const WORKTREES = ".claude/worktrees";
const CONFIG = /\.(json|jsonc|yml|yaml|toml)$/;

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

/**
 * Every config file under `repoRoot` (relative paths, sorted). `onFolder` is called with each folder opened
 * ("" for the root); the tests use it to check that skipped folders are never entered.
 */
export function configFiles(repoRoot: string, onFolder?: (folder: string) => void): string[] {
  const found: string[] = [];
  const walk = (folder: string) => {
    onFolder?.(folder);
    for (const entry of readdirSync(join(repoRoot, folder), { withFileTypes: true })) {
      const rel = folder ? `${folder}/${entry.name}` : entry.name;
      // A symlink is neither a file nor a folder here, so it is not followed.
      if (entry.isDirectory()) {
        if (!IGNORED.has(entry.name) && rel !== WORKTREES) walk(rel);
      } else if (entry.isFile() && CONFIG.test(entry.name)) {
        found.push(rel);
      }
    }
  };
  walk("");
  return found.sort();
}

/** Parse every file in `files` (by default, every config file under `repoRoot`). */
export function validateConfigs(
  repoRoot: string,
  files: string[] = configFiles(repoRoot),
): { problems: Problem[]; checked: number } {
  const problems: Problem[] = [];
  for (const file of files) {
    const message = parseProblem(file, readFileSync(join(repoRoot, file), "utf8"));
    if (message) problems.push({ file, message: message.split("\n")[0] as string });
  }
  return { problems, checked: files.length };
}

/**
 * The api purges the URLs the site caches (ADR 0010), so the two Workers must agree on the site's origin and the
 * region views. Compares site/wrangler.jsonc with workers/api/wrangler.jsonc (a missing value is the shared default).
 */
export function wranglerProblems(repoRoot: string): Problem[] {
  const read = (file: string) => {
    const parsed = Bun.JSONC.parse(readFileSync(join(repoRoot, file), "utf8")) as { vars?: Record<string, string> };
    return {
      NEWS_SITE_ORIGIN: (parsed.vars?.NEWS_SITE_ORIGIN ?? DEFAULT_SITE_ORIGIN).replace(/\/+$/, ""),
      NEWS_REGION_VIEWS: regionViews(parsed.vars?.NEWS_REGION_VIEWS).join(","),
    };
  };
  const site = read("site/wrangler.jsonc");
  const api = read("workers/api/wrangler.jsonc");
  return (Object.keys(site) as (keyof typeof site)[])
    .filter((k) => site[k] !== api[k])
    .map((k) => ({
      file: "site/wrangler.jsonc",
      message: `${k} differs from workers/api/wrangler.jsonc (${site[k]} vs ${api[k]})`,
    }));
}

if (import.meta.main) {
  const { problems, checked } = validateConfigs(root);
  problems.push(...wranglerProblems(root));
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
