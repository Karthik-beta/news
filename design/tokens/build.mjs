// Generates tokens.css from tokens.json. Run with `bun design/tokens/build.mjs` or `node design/tokens/build.mjs`.
// tokens.json is the source of truth; never edit tokens.css by hand.
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const tokens = JSON.parse(readFileSync(join(here, "tokens.json"), "utf8"));

const light = [];
const dark = [];

const fmt = (value, type) => {
  if (value && typeof value === "object" && !Array.isArray(value) && "value" in value) return `${value.value}${value.unit}`;
  if (Array.isArray(value)) return value.map(v => (/[\s]/.test(v) ? `"${v}"` : v)).join(", ");
  if (type === "fontFamily" && typeof value === "string") return value;
  return String(value);
};

function walk(node, path, inheritedType) {
  const type = node.$type ?? inheritedType;
  if ("$value" in node) {
    const name = `--${path.join("-")}`;
    light.push(`  ${name}: ${fmt(node.$value, type)};`);
    const d = node.$extensions?.["news.dark"];
    if (d !== undefined) dark.push(`  ${name}: ${fmt(d, type)};`);
    return;
  }
  for (const [k, v] of Object.entries(node)) {
    if (k.startsWith("$") || typeof v !== "object" || v === null) continue;
    walk(v, [...path, k], type);
  }
}
for (const [k, v] of Object.entries(tokens)) if (!k.startsWith("$")) walk(v, [k], undefined);

const darkBlock = dark.join("\n");
const css = `/* GENERATED from tokens.json by build.mjs. Do not edit by hand. */

:root {
  color-scheme: light;
${light.join("\n")}
}

@media (prefers-color-scheme: dark) {
  :root:not([data-theme="light"]) {
    color-scheme: dark;
${darkBlock.replace(/^/gm, "  ")}
  }
}

:root[data-theme="dark"] {
  color-scheme: dark;
${darkBlock}
}

/* Dark mode reverses the sequential anchor: use these for magnitude scales in dark mode (low -> high). */
@media (prefers-color-scheme: dark) {
  :root:not([data-theme="light"]) {
    --chart-seq-dark-low: var(--chart-seq-blue-700);
    --chart-seq-dark-high: var(--chart-seq-blue-100);
  }
}
:root[data-theme="dark"] {
  --chart-seq-dark-low: var(--chart-seq-blue-700);
  --chart-seq-dark-high: var(--chart-seq-blue-100);
}

/* Responsive values that tokens can't express as single numbers */
:root {
  --gutter: var(--layout-gutter-phone);
  --font-size-body-responsive: 1.0625rem;      /* 17px on phone */
  --font-size-standfirst-responsive: 1.125rem; /* 18px on phone */
  --font-size-figure-responsive: 1.5rem;       /* 24px on phone */
  --font-size-wordmark-responsive: 1.625rem;   /* 26px on phone */
}
@media (min-width: 600px) {
  :root {
    --gutter: var(--layout-gutter-tablet);
    --font-size-body-responsive: var(--font-size-body);
    --font-size-standfirst-responsive: var(--font-size-standfirst);
    --font-size-figure-responsive: var(--font-size-figure);
    --font-size-wordmark-responsive: var(--font-size-wordmark);
  }
}
@media (min-width: 980px) {
  :root { --gutter: var(--layout-gutter-desktop); }
}
@media (prefers-reduced-motion: reduce) {
  :root { --motion-fast: 0ms; --motion-base: 0ms; }
}
`;

writeFileSync(join(here, "tokens.css"), css);
console.log(`tokens.css written: ${light.length} tokens, ${dark.length} with dark values`);
