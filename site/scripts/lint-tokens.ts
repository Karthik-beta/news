// Token-only lint (Development Plan, "Quality gates"): no literal colours, font names or px font sizes anywhere in
// site/src outside the two files whose job is to define them (tokens.css, generated from tokens.json, and
// fonts.css, which names the self-hosted faces). Everything visual must come from a design token.
//   bun scripts/lint-tokens.ts        lint site/src, exit 1 with the findings
// The unit test runs the same lint over the tree and over planted examples, so CI fails on a violation.
import { readdirSync, readFileSync, statSync } from "node:fs";
import { dirname, extname, join, relative, sep } from "node:path";
import { fileURLToPath } from "node:url";

const site = join(dirname(fileURLToPath(import.meta.url)), "..");

export interface Finding {
  file: string;
  line: number;
  rule: "hex-colour" | "colour-function" | "font-name" | "font-family" | "px-font-size";
  text: string;
}

/** Files allowed to hold literal design values, relative to site/ with forward slashes. */
export const ALLOWED = new Set(["src/styles/tokens.css", "src/styles/fonts.css"]);
const EXTENSIONS = new Set([".css", ".ts", ".tsx", ".astro", ".mjs", ".js"]);

const HEX = /(?<![\w&/#-])#(?:[0-9a-fA-F]{8}|[0-9a-fA-F]{6}|[0-9a-fA-F]{3,4})(?![\w-])/g;
const COLOUR_FN = /\b(?:rgba?|hsla?|hwb|lab|lch|oklab|oklch|color)\(\s*[\d.]/g;
const FONT_NAMES =
  /\b(?:Literata|Fira Sans|Georgia|Arial|Helvetica(?: Neue)?|Roboto|Segoe UI|Times New Roman|Inter|Courier(?: New)?|Menlo|Consolas|Monaco|monospace|sans-serif|system-ui|ui-monospace|ui-serif|ui-sans-serif)\b/g;
const CSS_FONT_FAMILY = /font-family\s*:(?!\s*(?:var\(|(?:inherit|initial|unset|revert)\b))[^;}]+/g;
const CSS_PX_FONT = /font(?:-size)?\s*:[^;}]*?\b\d+(?:\.\d+)?px\b/g;
const JS_PX_FONT = /\bfont(?:Size|-size)\s*[:=]\s*\{?\s*["'`]?\d+(?:\.\d+)?(?:px)?\b/g;
const JS_FONT_FAMILY = /\bfontFamily\s*:\s*["'`](?!var\()/g;

/** Removes comments, keeping line breaks so line numbers stay right. */
function stripComments(text: string, css: boolean): string {
  const blank = (m: string) => m.replace(/[^\n]/g, " ");
  const noBlocks = text.replace(/\/\*[\s\S]*?\*\//g, blank);
  return css
    ? noBlocks
    : noBlocks.replace(/(^|[\s;{}(,])\/\/[^\n]*/g, (m, lead: string) => lead + blank(m.slice(lead.length)));
}

/** Lints one file's text. `file` decides the syntax (CSS or script/markup). */
export function lintSource(text: string, file: string): Finding[] {
  const css = file.endsWith(".css");
  const code = stripComments(text, css);
  const findings: Finding[] = [];
  const lineOf = (index: number) => code.slice(0, index).split("\n").length;
  const add = (rule: Finding["rule"], re: RegExp) => {
    for (const m of code.matchAll(re)) findings.push({ file, line: lineOf(m.index), rule, text: m[0].trim() });
  };
  add("hex-colour", HEX);
  add("colour-function", COLOUR_FN);
  add("font-name", FONT_NAMES);
  if (css) {
    add("font-family", CSS_FONT_FAMILY);
    add("px-font-size", CSS_PX_FONT);
  } else {
    add("font-family", JS_FONT_FAMILY);
    add("px-font-size", JS_PX_FONT);
    add("px-font-size", CSS_PX_FONT); // CSS inside <style> blocks and template strings
  }
  return findings.sort((a, b) => a.line - b.line);
}

/** Every lintable file under `dir` (site/src by default), relative to site/. Test files are linted too. */
export function lintableFiles(dir = join(site, "src")): string[] {
  const out: string[] = [];
  const walk = (d: string) => {
    for (const name of readdirSync(d)) {
      const path = join(d, name);
      if (statSync(path).isDirectory()) walk(path);
      else if (EXTENSIONS.has(extname(name))) out.push(relative(site, path).split(sep).join("/"));
    }
  };
  walk(dir);
  return out.filter((f) => !ALLOWED.has(f)).sort();
}

export function lintTree(dir?: string): Finding[] {
  return lintableFiles(dir).flatMap((f) => lintSource(readFileSync(join(site, f), "utf8"), f));
}

if (import.meta.main) {
  const findings = lintTree();
  for (const f of findings) console.error(`${f.file}:${f.line}  ${f.rule}  ${f.text}`);
  if (findings.length > 0) {
    console.error(`\n${findings.length} literal design value(s). Use the tokens in src/styles/tokens.css.`);
    process.exit(1);
  }
  console.log("token lint: no literal colours, font names or px font sizes");
}
