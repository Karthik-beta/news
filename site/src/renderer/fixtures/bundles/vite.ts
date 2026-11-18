// Fixture bundles for code that runs through Vite (fixture pages in the Astro build, Vitest).
import { assembleBundles, type RawBundle } from "./assemble";

const files = import.meta.glob<string>("./*/*.{oui,json}", { query: "?raw", import: "default", eager: true });

const raw: Record<string, RawBundle> = {};
for (const [path, text] of Object.entries(files)) {
  const [, name, file] = /^\.\/([^/]+)\/([^/]+)$/.exec(path) ?? [];
  if (!name || !file) continue;
  raw[name] ??= { oui: "", head: "" };
  const entry = raw[name];
  if (file === "page.oui") entry.oui = text;
  else if (file === "head.json") entry.head = text;
  else if (file === "data.json") entry.data = text;
  else if (file === "sources.json") entry.sources = text;
}

export const fixtureBundles = assembleBundles(raw);
