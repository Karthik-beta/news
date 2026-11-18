// bureau-single-entry (ADR 0008 §12): Chief is the only way a preset runs. Only Chief's sources (and
// tests) insert into `runs`, and only the service, Rota and tests call Chief's control functions (the
// Hotline handlers for Board, Rota for schedules).
import { describe, expect, test } from "bun:test";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";
import { root } from "../lib";

function files(dir: string, out: string[] = []): string[] {
  for (const name of readdirSync(dir)) {
    if (name === "target" || name === "node_modules" || name === "spikes") continue;
    const p = join(dir, name);
    if (statSync(p).isDirectory()) files(p, out);
    else if (p.endsWith(".rs") || p.endsWith(".sql")) out.push(p);
  }
  return out;
}

const sources = files(join(root, "cli")).map((p) => ({
  path: relative(root, p).replaceAll("\\", "/"),
  text: readFileSync(p, "utf8"),
}));
const isTest = (p: string) => /\/tests\//.test(p) || p.endsWith("/tests.rs");

describe("bureau-single-entry", () => {
  test("only news-chief (and tests) insert into runs", () => {
    const offenders = sources
      .filter((f) => /insert\s+into\s+runs\b/i.test(f.text))
      .map((f) => f.path)
      .filter((p) => !p.startsWith("cli/crates/news-chief/") && !isTest(p));
    expect(offenders).toEqual([]);
  });

  test("only the service, Rota and tests call Chief's control functions", () => {
    const control =
      /\.(start_after_pause|lineup_cancel|answer|finish_pause)\s*\(|\bchief\s*\.\s*(start|pause|resume|cancel)\s*\(/;
    const allowed = ["cli/crates/news-service/", "cli/crates/news-rota/", "cli/crates/news-chief/"];
    const offenders = sources
      .filter((f) => f.path.endsWith(".rs") && control.test(f.text))
      .map((f) => f.path)
      .filter((p) => !allowed.some((a) => p.startsWith(a)) && !isTest(p));
    expect(offenders).toEqual([]);
  });

  test("the check sees Chief's own insert", () => {
    expect(
      sources.some((f) => f.path.startsWith("cli/crates/news-chief/") && /insert\s+into\s+runs\b/i.test(f.text)),
    ).toBe(true);
  });
});
