/// <reference types="bun" />
// Measures approach B (OpenUI Renderer.Root + react-dom/server) and C (own walker over the parse tree) in Bun:
// render time p50/p95 over 50 renders (parse included), HTML size, and the island JS a page ships.
// Run from site/: bun --bun spikes/openui-ssr/measure-react.ts
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { renderDocument, renderWithOpenUI, renderWithOwnWalker } from "./render";
import { bench, sizes, writeResults } from "./stats";

const here = fileURLToPath(new URL(".", import.meta.url));
const oui = await Bun.file(join(here, "fixture.oui")).text();
const doc = (body: string) =>
  renderDocument(body, { title: "OpenUI SSR spike", css: "/page.css", script: "/islands.js" });

async function bundle(entry: string) {
  const outdir = mkdtempSync(join(tmpdir(), "openui-ssr-"));
  try {
    const out = await Bun.build({
      entrypoints: [join(here, entry)],
      outdir,
      target: "browser",
      minify: true,
      define: { "process.env.NODE_ENV": '"production"' },
    });
    if (!out.success) throw new Error(out.logs.join("\n"));
    const js = await Promise.all(out.outputs.filter((o) => o.kind === "entry-point").map((o) => o.text()));
    return sizes(js.join(""));
  } finally {
    rmSync(outdir, { recursive: true, force: true });
  }
}

const b = await bench(() => doc(renderWithOpenUI(oui)));
const c = await bench(() => doc(renderWithOwnWalker(oui)));
const html = doc(renderWithOpenUI(oui));
if (html !== doc(renderWithOwnWalker(oui))) throw new Error("B and C rendered different HTML");

const results = {
  machine: { runtime: `bun ${Bun.version}`, platform: process.platform, arch: process.arch },
  B_reactServer: { render: b, html: sizes(html), islandJs: await bundle("hydrate.client.ts") },
  C_ownWalker: { render: c, html: sizes(html), islandJs: await bundle("hydrate.client.ts") },
  // What hydrating the whole page with OpenUI's runtime would cost instead of islands (for the ADR).
  wholePageHydrationJs: await bundle("whole-page.client.tsx"),
};
writeResults(join(here, "results.json"), results);
console.log(JSON.stringify(results, null, 2));
