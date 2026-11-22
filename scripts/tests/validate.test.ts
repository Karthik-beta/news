// Tests for scripts/validate-skills.ts and scripts/validate-config.ts, on known-good and known-bad input,
// plus a check that the real repo passes both (so a bad skill or config file fails `bun test scripts/tests` too).
import { afterEach, describe, expect, test } from "bun:test";
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { root } from "../lib";
import { configFiles, isLenientJson, parseProblem, validateConfigs, wranglerProblems } from "../validate-config";
import { checkFile, parseFrontmatter, validateRepo } from "../validate-skills";
import { removeDir, tempDir } from "./helpers";

const skill = (name: string, description = "Does a thing.") =>
  `---\nname: ${name}\ndescription: ${description}\n---\n\n# Body\n`;

const dirs: string[] = [];
afterEach(() => {
  for (const d of dirs.splice(0)) removeDir(d);
});
function fixture(files: Record<string, string>): string {
  const dir = tempDir("news-validate-");
  dirs.push(dir);
  for (const [rel, text] of Object.entries(files)) {
    mkdirSync(dirname(join(dir, rel)), { recursive: true });
    writeFileSync(join(dir, rel), text);
  }
  return dir;
}

describe("parseFrontmatter", () => {
  test("reads a name and description, with LF, CRLF and a BOM", () => {
    for (const text of [skill("a"), skill("a").replaceAll("\n", "\r\n"), `\uFEFF${skill("a")}`]) {
      const fm = parseFrontmatter(text);
      expect("data" in fm && fm.data.name).toBe("a");
    }
  });

  test("reads quoted descriptions with escapes, colons and multi-line folded text", () => {
    const quoted = parseFrontmatter('---\nname: a\ndescription: "Use it: when \\"x\\" happens"\n---\n');
    expect("data" in quoted && quoted.data.description).toBe('Use it: when "x" happens');
    const folded = parseFrontmatter("---\nname: a\ndescription: >\n  one\n  two\n---\n");
    expect("data" in folded && String(folded.data.description).trim()).toBe("one two");
  });

  test("rejects a file with no frontmatter, an unclosed block, invalid YAML and a non-mapping", () => {
    for (const text of ["# Title\n", "---\nname: a\n", "---\nname: [oops\n---\n", "---\n- a\n- b\n---\n", ""]) {
      expect("error" in parseFrontmatter(text), JSON.stringify(text)).toBe(true);
    }
  });

  test("an unquoted colon in a description is invalid YAML and is reported, not swallowed", () => {
    const fm = parseFrontmatter("---\nname: a\ndescription: Use this: always\n---\n");
    expect("error" in fm).toBe(true);
  });
});

describe("checkFile", () => {
  test("a valid skill and a valid agent pass", () => {
    expect(checkFile("s/SKILL.md", skill("news-thing"), "skill", "news-thing")).toEqual([]);
    expect(checkFile("a.md", "---\nname: architect\ndescription: Plans.\nmodel: opus\n---\nBody", "agent")).toEqual([]);
  });

  test("a missing, empty or non-string name or description is reported", () => {
    const cases = [
      "---\ndescription: x\n---\n",
      "---\nname: a\n---\n",
      '---\nname: ""\ndescription: x\n---\n',
      '---\nname: a\ndescription: "   "\n---\n',
      "---\nname: 5\ndescription: x\n---\n",
      "---\nname: a\ndescription: [x]\n---\n",
    ];
    for (const text of cases) expect(checkFile("a.md", text, "agent").length, text).toBeGreaterThan(0);
  });

  test("a skill name must match its folder", () => {
    const problems = checkFile("skills/foo/SKILL.md", skill("bar"), "skill", "foo");
    expect(problems.map((p) => p.message)).toEqual([`skill name "bar" must match its folder "foo"`]);
  });

  test("a skill name must be lowercase kebab-case; an agent name is not held to it", () => {
    expect(checkFile("s/SKILL.md", skill("Bad_Name"), "skill", "Bad_Name").length).toBe(1);
    expect(checkFile("a.md", skill("Bad_Name"), "agent")).toEqual([]);
  });

  test("a skill description over 1024 characters is reported", () => {
    expect(checkFile("s/SKILL.md", skill("a", "x".repeat(1025)), "skill", "a").length).toBe(1);
    expect(checkFile("s/SKILL.md", skill("a", "x".repeat(1024)), "skill", "a")).toEqual([]);
  });
});

describe("validateRepo", () => {
  test("finds skills in both skill roots and agents, and passes when all are valid", () => {
    const dir = fixture({
      ".claude/skills/dev-one/SKILL.md": skill("dev-one"),
      "skills/page-one/SKILL.md": skill("page-one"),
      "skills/README.md": "not a skill, no frontmatter needed",
      ".claude/agents/builder.md": skill("builder"),
    });
    expect(validateRepo(dir)).toEqual({ problems: [], checked: 3 });
  });

  test("reports every bad file, naming each one, and a skill folder without SKILL.md", () => {
    const dir = fixture({
      ".claude/skills/good/SKILL.md": skill("good"),
      ".claude/skills/mismatch/SKILL.md": skill("other"),
      "skills/empty/notes.md": "no skill file here",
      "skills/nofm/SKILL.md": "# no frontmatter",
      ".claude/agents/broken.md": "---\nname: broken\n---\n",
      ".claude/agents/notes.txt": "ignored, not markdown",
    });
    const { problems, checked } = validateRepo(dir);
    expect(checked).toBe(4);
    expect(problems.map((p) => p.file).sort()).toEqual([
      ".claude/agents/broken.md",
      ".claude/skills/mismatch/SKILL.md",
      "skills/empty",
      "skills/nofm/SKILL.md",
    ]);
  });

  test("a repo with neither root checks nothing, without throwing", () => {
    expect(validateRepo(fixture({ "README.md": "x" }))).toEqual({ problems: [], checked: 0 });
  });

  test("the real repo's skills and agents are valid", () => {
    const { problems, checked } = validateRepo(root);
    expect(problems).toEqual([]);
    expect(checked).toBeGreaterThan(10);
  });
});

describe("parseProblem", () => {
  test("strict JSON rejects comments and trailing commas; tsconfig, biome and .jsonc allow them", () => {
    const lenient = '{ // note\n "a": 1, }';
    expect(parseProblem("package.json", lenient)).toBeDefined();
    expect(parseProblem("tokens.json", lenient)).toBeDefined();
    expect(parseProblem("site/tsconfig.json", lenient)).toBeUndefined();
    expect(parseProblem("tsconfig.build.json", lenient)).toBeUndefined();
    expect(parseProblem("biome.json", lenient)).toBeUndefined();
    expect(parseProblem("workers/api/wrangler.jsonc", lenient)).toBeUndefined();
  });

  test("broken JSON, YAML and TOML are reported; good ones pass", () => {
    expect(parseProblem("a.json", '{"a": }')).toBeDefined();
    expect(parseProblem("a.json", '{"a": 1}')).toBeUndefined();
    expect(parseProblem("a.yml", "a: [")).toBeDefined();
    expect(parseProblem("a.yml", "a:\n  - 1\n  - 2\n")).toBeUndefined();
    expect(parseProblem("a.yaml", "a: 1\n b: 2\n")).toBeDefined();
    expect(parseProblem("a.toml", "a = ")).toBeDefined();
    expect(parseProblem("a.toml", '[package]\nname = "x"\n')).toBeUndefined();
  });

  test("a workflow with GitHub expressions parses as YAML", () => {
    const expression = "$" + "{{ github.ref == 'x' }}"; // built in two parts so the linter does not read it as a template
    expect(parseProblem("w.yml", `on: push\njobs:\n  a:\n    if: ${expression}\n`)).toBeUndefined();
  });

  test("isLenientJson only matches the intended names", () => {
    expect(isLenientJson("a/tsconfig.json")).toBe(true);
    expect(isLenientJson("a/my-tsconfig.json")).toBe(false);
    expect(isLenientJson("a/package.json")).toBe(false);
  });
});

describe("wranglerProblems (site and api agree on the origin and the region views)", () => {
  const wrangler = (vars: Record<string, string>) => `{\n // comment\n "vars": ${JSON.stringify(vars)},\n}`;
  const both = (site: Record<string, string>, api: Record<string, string>) =>
    fixture({ "site/wrangler.jsonc": wrangler(site), "workers/api/wrangler.jsonc": wrangler(api) });

  test("equal values, or a value that is the shared default, pass", () => {
    const same = { NEWS_SITE_ORIGIN: "https://news.karthik.nz", NEWS_REGION_VIEWS: "europe,india" };
    expect(wranglerProblems(both(same, same))).toEqual([]);
    expect(wranglerProblems(both({ NEWS_SITE_ORIGIN: "https://news.karthik.nz/" }, {}))).toEqual([]);
  });

  test("a different origin or region list is reported", () => {
    const problems = wranglerProblems(
      both(
        { NEWS_SITE_ORIGIN: "https://example.org", NEWS_REGION_VIEWS: "europe" },
        { NEWS_SITE_ORIGIN: "https://news.karthik.nz", NEWS_REGION_VIEWS: "europe,india" },
      ),
    );
    expect(problems.map((p) => p.message.split(" differs")[0])).toEqual(["NEWS_SITE_ORIGIN", "NEWS_REGION_VIEWS"]);
  });

  test("the real repo's two wrangler files agree", () => {
    expect(wranglerProblems(root)).toEqual([]);
  });
});

describe("validateConfigs", () => {
  test("skips node_modules, build output and other worktrees, and finds nested configs", () => {
    const dir = fixture({
      "a/b/ok.yml": "a: 1\n",
      "node_modules/x/bad.json": "{",
      "cli/target/bad.toml": "a =",
      "site/dist/bad.json": "{",
      ".claude/worktrees/agent-1/bad.yml": "a: [",
      ".github/workflows/ci.yml": "on: push\n",
    });
    expect(configFiles(dir)).toEqual([".github/workflows/ci.yml", "a/b/ok.yml"]);
    expect(validateConfigs(dir)).toEqual({ problems: [], checked: 2 });
  });

  test("does not descend into skipped folders, so a big node_modules or another worktree costs nothing", () => {
    const dir = fixture({
      "ok.json": "{}",
      "node_modules/x/deep/a.json": "{",
      ".claude/worktrees/agent-1/site/b.json": "{",
      "cli/target/debug/c.toml": "a =",
      "site/node_modules/y/d.json": "{",
    });
    const opened: string[] = [];
    expect(configFiles(dir, (folder) => opened.push(folder))).toEqual(["ok.json"]);
    expect(opened.sort()).toEqual(["", ".claude", "cli", "site"]);
  });

  test("names each broken file", () => {
    const dir = fixture({ "good.json": "{}", "bad.json": "{", "bad.yml": "a: [" });
    const { problems, checked } = validateConfigs(dir);
    expect(checked).toBe(3);
    expect(problems.map((p) => p.file).sort()).toEqual(["bad.json", "bad.yml"]);
  });

  test("the real repo's config files parse, and include the ones CI cares about", () => {
    // One walk of the repo, shared by both checks.
    const files = configFiles(root);
    const { problems } = validateConfigs(root, files);
    expect(problems).toEqual([]);
    for (const must of [
      "design/tokens/tokens.json",
      "workers/api/wrangler.jsonc",
      "site/tsconfig.json",
      ".github/ISSUE_TEMPLATE/2-bug.yml",
      ".github/workflows/ci.yml",
      "cli/deny.toml",
    ])
      expect(files, must).toContain(must);
  });
});
