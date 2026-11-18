import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { componentFolders, folderProblems, generate } from "../../scripts/gen-components";
import { type AnyComponentSpec, estimateTokens } from "../../src/components/contract";
import {
  buildComponentIndex,
  componentSpecs,
  createNewsLibrary,
  indexBudgetProblems,
  newsLibrary,
} from "../../src/components/library";
import { specs } from "../../src/components/registry.generated";
import { fixtureSpecs } from "../../src/renderer/fixtures/library";
import { siteRoot } from "../support/tokens";

describe("component folder contract", () => {
  it("every folder has <Name>.tsx, spec.ts, fixtures/ and SIGNOFF.md, and its spec names it", () => {
    const dir = join(siteRoot, "src/components");
    expect(componentFolders(dir).length).toBeGreaterThan(0);
    expect(componentFolders(dir).flatMap((name) => folderProblems(dir, name))).toEqual([]);
  });

  it("the checker reports a folder that breaks the contract", () => {
    const dir = mkdtempSync(join(tmpdir(), "news-components-"));
    mkdirSync(join(dir, "Broken"));
    writeFileSync(join(dir, "Broken", "spec.ts"), 'export default definePrimitive({ name: "Other" });');
    const problems = folderProblems(dir, "Broken");
    expect(problems).toContain("Broken: missing Broken.tsx");
    expect(problems).toContain("Broken: missing SIGNOFF.md");
    expect(problems).toContain("Broken: fixtures/ is missing or empty");
    expect(problems).toContain('Broken: spec.ts name must be "Broken"');
  });

  it("the generated registry, island list and stylesheet are up to date (bun scripts/gen-components.ts)", () => {
    const { files, problems } = generate();
    expect(problems).toEqual([]);
    for (const [path, text] of Object.entries(files)) {
      expect(readFileSync(join(siteRoot, path), "utf8").replaceAll("\r\n", "\n"), path).toBe(text);
    }
  });
});

describe("generated OpenUI library", () => {
  it("is built from the folders' specs, with Page as the root", () => {
    const { library, specs: byName } = newsLibrary();
    expect(library.root).toBe("Page");
    expect([...byName.keys()].sort()).toEqual(
      componentSpecs(specs)
        .map((s) => s.name)
        .sort(),
    );
  });

  it("puts every non-root component in Page's children, so the parser accepts it", () => {
    const { library } = createNewsLibrary([...componentSpecs(specs), ...fixtureSpecs]);
    const prompt = library.prompt();
    expect(prompt).toContain("Page(children: (FixtureText | FixtureFigure | FixtureLineChart)[])");
  });

  it("refuses two roots or a duplicate name", () => {
    const page = componentSpecs(specs).find((s) => s.name === "Page") as AnyComponentSpec;
    expect(() => createNewsLibrary([page, page])).toThrow(/exactly one root/);
    const text = fixtureSpecs[0] as AnyComponentSpec;
    expect(() => createNewsLibrary([page, text, text])).toThrow(/Two components/);
  });
});

describe("component index (ADR 0001 budget)", () => {
  it("fits 4,000 tokens, with no entry over 24", () => {
    const index = buildComponentIndex(componentSpecs(specs));
    expect(indexBudgetProblems(index)).toEqual([]);
    expect(index.tokens).toBeLessThanOrEqual(4000);
  });

  it("every entry reads 'Name — purpose — data shape' and starts with its name", () => {
    for (const s of [...componentSpecs(specs), ...fixtureSpecs]) {
      expect(s.index, s.name).toMatch(new RegExp(`^${s.name} — [^—]+ — [^—]+$`));
      expect(estimateTokens(s.index), s.name).toBeLessThanOrEqual(24);
    }
  });

  it("flags an entry and an index over budget", () => {
    const long = { ...(fixtureSpecs[0] as AnyComponentSpec), index: `Long — ${"word ".repeat(30)}— x` };
    expect(indexBudgetProblems(buildComponentIndex([long]))[0]).toMatch(/over the 24-token entry budget/);
    const many = Array.from({ length: 700 }, (_, i) => ({ ...long, name: `C${i}`, index: `C${i} — a b c — d e` }));
    expect(indexBudgetProblems(buildComponentIndex(many)).at(-1)).toMatch(/over the 4000-token budget/);
  });

  it("counts tokens as ceil(UTF-8 bytes / 3)", () => {
    expect(estimateTokens("abc")).toBe(1);
    expect(estimateTokens("abcd")).toBe(2);
    expect(estimateTokens("—")).toBe(1); // 3 bytes
  });
});
