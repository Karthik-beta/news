// Fixture bundles read from disk, for Bun scripts and the Bun SSR tests (no Vite there).
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { assembleBundles, type RawBundle } from "./assemble";

const here = dirname(fileURLToPath(import.meta.url));

export function readFixtureBundles() {
  const raw: Record<string, RawBundle> = {};
  for (const name of readdirSync(here, { withFileTypes: true })) {
    if (!name.isDirectory()) continue;
    const at = (f: string) => join(here, name.name, f);
    const read = (f: string) => (existsSync(at(f)) ? readFileSync(at(f), "utf8") : undefined);
    raw[name.name] = {
      oui: read("page.oui") ?? "",
      head: read("head.json") ?? "",
      data: read("data.json"),
      sources: read("sources.json"),
    };
  }
  return assembleBundles(raw);
}
