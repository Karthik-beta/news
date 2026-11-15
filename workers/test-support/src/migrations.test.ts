// Guards for the harness D1 migration folder. The migration reader splits statements on semicolons and orders
// files by number, so a stray semicolon in a comment or a gap in the numbering breaks the integration tests quietly.
import { readdirSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const dir = fileURLToPath(new URL("../d1-migrations", import.meta.url));
const files = readdirSync(dir).sort();

describe("d1-migrations", () => {
  it("has at least one migration", () => {
    expect(files.length).toBeGreaterThan(0);
  });

  it("uses NNNN_name.sql names, numbered 0001, 0002, ... with no gaps", () => {
    for (const name of files) expect(name).toMatch(/^\d{4}_[a-z0-9_]+\.sql$/);
    expect(files.map((f) => Number(f.slice(0, 4)))).toEqual(files.map((_, i) => i + 1));
  });

  it.each(files)("%s has no semicolons inside comments and ends every statement with one", (name) => {
    const sql = readFileSync(`${dir}/${name}`, "utf8");
    for (const line of sql.split("\n")) {
      if (line.trimStart().startsWith("--")) expect(line).not.toContain(";");
    }
    const statements = sql
      .split("\n")
      .filter((l) => !l.trimStart().startsWith("--"))
      .join("\n")
      .trim();
    expect(statements.length).toBeGreaterThan(0);
    expect(statements.endsWith(";")).toBe(true);
  });

  it("states that it is not the production search schema", () => {
    expect(readFileSync(`${dir}/${files[0]}`, "utf8")).toMatch(/NOT the production search schema/);
  });
});
