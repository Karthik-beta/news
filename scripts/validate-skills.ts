// Checks the YAML frontmatter of every skill and agent file. Run from the repo root: `bun scripts/validate-skills.ts`.
//   skills:  .claude/skills/<name>/SKILL.md and skills/<name>/SKILL.md (runtime skills for page writers)
//   agents:  .claude/agents/<name>.md
// Each needs frontmatter with a non-empty `name` and `description`; a skill's `name` must equal its folder name.
// Runtime skills also pass the page-writer lint (`runtimeSkillProblems`).
// Exits 1 and lists every problem (not just the first) so one CI run shows everything to fix.
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { root } from "./lib";

export interface Problem {
  file: string;
  message: string;
}

export type Frontmatter = { data: Record<string, unknown> } | { error: string };

/** Parse the `---` fenced YAML block at the very start of a Markdown file. */
export function parseFrontmatter(text: string): Frontmatter {
  const source = text.replace(/^﻿/, "").replace(/\r\n/g, "\n");
  const match = source.match(/^---[ \t]*\n([\s\S]*?)\n---[ \t]*(?:\n|$)/);
  if (!match) return { error: "no frontmatter: the file must start with a --- fenced YAML block" };
  let data: unknown;
  try {
    data = Bun.YAML.parse(match[1] as string);
  } catch (e) {
    return { error: `frontmatter is not valid YAML (${e instanceof Error ? e.message : String(e)})` };
  }
  if (data === null || typeof data !== "object" || Array.isArray(data))
    return { error: "frontmatter must be a YAML mapping of keys to values" };
  return { data: data as Record<string, unknown> };
}

const SKILL_NAME = /^[a-z0-9]+(?:-[a-z0-9]+)*$/;
const MAX_SKILL_NAME = 64;
const MAX_DESCRIPTION = 1024;

function nonEmptyString(value: unknown): value is string {
  return typeof value === "string" && value.trim() !== "";
}

/**
 * Problems in one skill or agent file. `file` is the path shown in messages; `folder` is the skill's
 * directory name (skills only), which `name` must equal.
 */
export function checkFile(file: string, text: string, kind: "skill" | "agent", folder?: string): Problem[] {
  const fm = parseFrontmatter(text);
  if ("error" in fm) return [{ file, message: fm.error }];
  const problems: Problem[] = [];
  const { name, description } = fm.data;
  if (!nonEmptyString(name)) problems.push({ file, message: "frontmatter needs a non-empty `name` (a string)" });
  if (!nonEmptyString(description))
    problems.push({ file, message: "frontmatter needs a non-empty `description` (a string)" });
  if (kind === "skill" && nonEmptyString(name)) {
    if (name !== folder) problems.push({ file, message: `skill name "${name}" must match its folder "${folder}"` });
    if (!SKILL_NAME.test(name) || name.length > MAX_SKILL_NAME)
      problems.push({
        file,
        message: `skill name "${name}" must be lowercase letters, digits and single hyphens, at most ${MAX_SKILL_NAME} characters`,
      });
  }
  if (kind === "skill" && nonEmptyString(description) && description.length > MAX_DESCRIPTION)
    problems.push({
      file,
      message: `description is ${description.length} characters; the limit for a skill is ${MAX_DESCRIPTION}`,
    });
  return problems;
}

// Runtime skills (`skills/`) also pass the page-writer lint (#19): `news-brand` is at most 1,000 words, every
// component a skill names is in the component index, and no example contains a digit outside a ref.
const BRAND_MAX_WORDS = 1000;
const REF = /\{?\b(?:dp|dv|pd|cx|q|sr)_[A-Za-z0-9_…]*\}?/g;
const FENCE = /^```[^\n]*\n([\s\S]*?)^```/gm;

/** The component index's names: component folders whose spec is a page component or the root, not a primitive. */
export function indexedComponents(repoRoot: string): Set<string> {
  const dir = join(repoRoot, "site/src/components");
  return new Set(
    subdirectories(dir).filter((name) => {
      const spec = join(dir, name, "spec.ts");
      return (
        existsSync(spec) && /export default define(?:NewsComponent|RootComponent)\(/.test(readFileSync(spec, "utf8"))
      );
    }),
  );
}

/**
 * Lint problems in one runtime skill. Components named are backticked PascalCase names and `Name(` calls in code;
 * examples are fenced blocks and quoted strings in the prose. An empty `components` set skips that check (a repo
 * with no component folders).
 */
export function runtimeSkillProblems(file: string, text: string, folder: string, components: Set<string>): Problem[] {
  const body = text
    .replace(/^﻿/, "")
    .replace(/\r\n/g, "\n")
    .replace(/^---[ \t]*\n[\s\S]*?\n---[ \t]*(?:\n|$)/, "");
  const problems: Problem[] = [];
  if (folder === "news-brand") {
    const words = body.split(/\s+/).filter(Boolean).length;
    if (words > BRAND_MAX_WORDS)
      problems.push({ file, message: `news-brand is ${words} words; the limit is ${BRAND_MAX_WORDS}` });
  }
  const fences = [...body.matchAll(FENCE)].map((m) => m[1] as string);
  const prose = body.replace(FENCE, "");
  const spans = [...prose.matchAll(/`([^`\n]+)`/g)].map((m) => m[1] as string);
  if (components.size > 0) {
    const named = new Set(spans.filter((s) => /^[A-Z][A-Za-z0-9]*$/.test(s)));
    for (const code of [...fences, ...spans])
      for (const m of code.matchAll(/\b([A-Z][A-Za-z0-9]*)\(/g)) named.add(m[1] as string);
    for (const name of [...named].sort())
      if (!components.has(name))
        problems.push({ file, message: `names component \`${name}\`, which is not in the component index` });
  }
  const quotes = [...prose.replace(/`[^`\n]+`/g, "").matchAll(/"([^"\n]*)"|“([^”\n]*)”/g)].map(
    (m) => (m[1] ?? m[2]) as string,
  );
  for (const example of [...fences, ...quotes])
    if (/\d/.test(example.replace(REF, "")))
      problems.push({ file, message: `an example has a digit outside a ref: ${example.trim().slice(0, 80)}` });
  return problems;
}

function subdirectories(dir: string): string[] {
  if (!existsSync(dir)) return [];
  return readdirSync(dir, { withFileTypes: true })
    .filter((e) => e.isDirectory())
    .map((e) => e.name)
    .sort();
}

/** Every skill and agent file under `repoRoot`, checked. Missing roots are fine (a repo may have none). */
export function validateRepo(repoRoot: string): { problems: Problem[]; checked: number } {
  const problems: Problem[] = [];
  let checked = 0;
  const components = indexedComponents(repoRoot);

  for (const base of [".claude/skills", "skills"]) {
    for (const folder of subdirectories(join(repoRoot, base))) {
      const rel = `${base}/${folder}/SKILL.md`;
      const path = join(repoRoot, rel);
      if (!existsSync(path)) {
        problems.push({ file: `${base}/${folder}`, message: "skill folder has no SKILL.md" });
        continue;
      }
      checked++;
      const text = readFileSync(path, "utf8");
      problems.push(...checkFile(rel, text, "skill", folder));
      if (base === "skills") problems.push(...runtimeSkillProblems(rel, text, folder, components));
    }
  }

  const agentsDir = join(repoRoot, ".claude/agents");
  if (existsSync(agentsDir)) {
    for (const entry of readdirSync(agentsDir).sort()) {
      if (!entry.endsWith(".md")) continue;
      checked++;
      problems.push(...checkFile(`.claude/agents/${entry}`, readFileSync(join(agentsDir, entry), "utf8"), "agent"));
    }
  }
  return { problems, checked };
}

if (import.meta.main) {
  const { problems, checked } = validateRepo(root);
  if (checked === 0) {
    console.error("No skill or agent files found: is this the repo root?");
    process.exit(1);
  }
  if (problems.length > 0) {
    for (const p of problems) console.error(`${p.file}: ${p.message}`);
    console.error(`\n${problems.length} problem(s) in ${checked} skill and agent file(s).`);
    process.exit(1);
  }
  console.log(`OK: ${checked} skill and agent files have valid frontmatter.`);
}
