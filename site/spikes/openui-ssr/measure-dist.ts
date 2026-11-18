// Measures what each built fixture page ships: HTML bytes, and all JavaScript a browser would fetch or run
// for it (inline scripts, module scripts, Astro island component and renderer URLs, and their static and
// dynamic imports), raw and gzip. Needs a build first.
// Run from site/: bun run build && bun --bun spikes/openui-ssr/measure-dist.ts
import { existsSync, readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { sizes, writeResults } from "./stats";

const here = fileURLToPath(new URL(".", import.meta.url));
const dist = resolve(here, "../../dist");
const pages = { A_astroIslands: "fixtures/openui-ssr/astro-islands", B_reactServer: "fixtures/openui-ssr/react-ssr" };

function jsGraph(entryUrls: string[]): string[] {
  const seen = new Set<string>();
  const queue = entryUrls.map((u) => join(dist, u));
  while (queue.length) {
    const file = queue.pop() as string;
    if (seen.has(file) || !existsSync(file)) continue;
    seen.add(file);
    const src = readFileSync(file, "utf8");
    for (const m of src.matchAll(/(?:from|import)\s*\(?\s*["'](\.{1,2}\/[^"']+\.js)["']/g)) {
      queue.push(resolve(dirname(file), m[1] as string));
    }
  }
  return [...seen];
}

const out: Record<string, unknown> = {};
for (const [name, path] of Object.entries(pages)) {
  const file = join(dist, path, "index.html");
  if (!existsSync(file)) {
    console.warn(`skipped ${name}: ${path} not built (approach A needs OPENUI_SPIKE_A=1 at build time)`);
    continue;
  }
  const html = readFileSync(file, "utf8");
  const inline = [
    ...html.matchAll(/<script(?![^>]*\bsrc=)(?![^>]*application\/ld\+json)[^>]*>([\s\S]*?)<\/script>/g),
  ].map((m) => m[1] as string);
  const urls = [
    ...[...html.matchAll(/<script[^>]*\bsrc="([^"]+)"/g)].map((m) => m[1] as string),
    ...[...html.matchAll(/\b(?:component-url|renderer-url|before-hydration-url)="([^"]+)"/g)].map(
      (m) => m[1] as string,
    ),
  ];
  const files = jsGraph(urls);
  const external = files.map((f) => readFileSync(f, "utf8")).join("");
  out[`dist_${name}`] = {
    html: sizes(html),
    inlineJs: sizes(inline.join("")),
    externalJs: sizes(external),
    totalJs: sizes(inline.join("") + external),
    jsFiles: files.map((f) => f.slice(dist.length + 1).replaceAll("\\", "/")),
  };
}
writeResults(join(here, "results.json"), out);
console.log(JSON.stringify(out, null, 2));
