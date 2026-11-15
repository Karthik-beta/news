// Writes contracts/bundle/data.schema.json and sources.schema.json (JSON Schema 2020-12) from the zod schemas in
// src/renderer/bundle/schema.ts. Run `bun run contracts` after changing a schema; a unit test fails while the
// committed files differ from what this would write.
import { writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { z } from "zod/v4";
import { PageData, PageSources } from "../src/renderer/bundle/schema";

const repo = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const BASE = "https://news.karthik.nz/contracts/bundle/";

export function contractSchemas(): Record<string, string> {
  const out: Record<string, string> = {};
  for (const [file, schema] of [
    ["data.schema.json", PageData],
    ["sources.schema.json", PageSources],
  ] as const) {
    const json = { ...z.toJSONSchema(schema, { target: "draft-2020-12", io: "input" }), $id: `${BASE}${file}` };
    out[`contracts/bundle/${file}`] = `${JSON.stringify(json, null, 2)}\n`;
  }
  return out;
}

if (import.meta.main) {
  for (const [path, text] of Object.entries(contractSchemas())) {
    writeFileSync(join(repo, path), text);
    console.log(`wrote ${path}`);
  }
}
