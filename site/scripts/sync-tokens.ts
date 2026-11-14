// Copies the generated design tokens into the site. tokens.json is the source of truth;
// design/tokens/build.mjs generates tokens.css; this script copies it here. Never edit the copy.

import { spawnSync } from "node:child_process";
import { copyFileSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const build = spawnSync(process.execPath, [join(root, "design/tokens/build.mjs")], { stdio: "inherit" });
if (build.status !== 0) process.exit(build.status ?? 1);

const target = join(root, "site/src/styles/tokens.css");
copyFileSync(join(root, "design/tokens/tokens.css"), target);
const css = readFileSync(target, "utf8");
writeFileSync(
  target,
  css.replace(
    "/* GENERATED from tokens.json by build.mjs.",
    "/* COPIED from design/tokens/tokens.css by site/scripts/sync-tokens.ts; GENERATED from tokens.json by build.mjs.",
  ),
);
console.log("tokens.css synced to site/src/styles/");
