// Reads design/tokens/tokens.json (the source of truth) into a flat list of CSS custom properties.
// Written independently of design/tokens/build.mjs on purpose: the tests check the generator's output
// against this reading of the spec, so a bug in the generator cannot hide behind itself.
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

export const siteRoot = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
export const repoRoot = join(siteRoot, "..");

export interface Token {
  /** CSS custom property name, e.g. `--color-paper`. */
  name: string;
  /** Value in the light theme (and the only value for theme-independent tokens). */
  light: string;
  /** Value in the dark theme, when the token has one. */
  dark?: string;
}

type TokenNode = { $value?: unknown; $type?: string; $extensions?: Record<string, unknown> } & Record<string, unknown>;

function format(value: unknown): string {
  if (Array.isArray(value)) return value.map((v) => (/\s/.test(String(v)) ? `"${v}"` : String(v))).join(", ");
  if (value && typeof value === "object" && "value" in value && "unit" in value) {
    const { value: n, unit } = value as { value: number; unit: string };
    return `${n}${unit}`;
  }
  return String(value);
}

export function readTokens(): Token[] {
  const json = JSON.parse(readFileSync(join(repoRoot, "design/tokens/tokens.json"), "utf8")) as Record<string, unknown>;
  const out: Token[] = [];
  const walk = (node: TokenNode, path: string[]) => {
    if ("$value" in node) {
      const dark = node.$extensions?.["news.dark"];
      out.push({
        name: `--${path.join("-")}`,
        light: format(node.$value),
        ...(dark === undefined ? {} : { dark: format(dark) }),
      });
      return;
    }
    for (const [key, child] of Object.entries(node)) {
      if (key.startsWith("$") || typeof child !== "object" || child === null) continue;
      walk(child as TokenNode, [...path, key]);
    }
  };
  for (const [key, child] of Object.entries(json)) {
    if (!key.startsWith("$") && typeof child === "object" && child !== null) walk(child as TokenNode, [key]);
  }
  return out;
}

export function tokenValue(name: string, theme: "light" | "dark" = "light"): string {
  const token = readTokens().find((t) => t.name === name);
  if (!token) throw new Error(`No token ${name} in tokens.json`);
  return theme === "dark" ? (token.dark ?? token.light) : token.light;
}

/**
 * Collects the custom-property declarations of every rule whose selector list contains `selector`, inside
 * the given at-rule context (`""` = top level, or e.g. `@media (prefers-color-scheme: dark)`). Selectors and
 * media queries are compared after removing whitespace and quotes, so readable and minified CSS both work.
 */
export function declarationsFor(css: string, selector: string, media = ""): Map<string, string> {
  const wantSelector = squash(selector);
  const wantMedia = squash(media);
  const result = new Map<string, string>();
  const text = css.replace(/\/\*[\s\S]*?\*\//g, "");
  const stack: string[] = [];
  let buffer = "";
  for (const ch of text) {
    if (ch === "{") {
      stack.push(buffer.trim());
      buffer = "";
    } else if (ch === "}") {
      const prelude = stack.pop() ?? "";
      const context = squash(stack.join(" "));
      const selectors = prelude.split(",").map(squash);
      if (context === wantMedia && selectors.includes(wantSelector)) {
        for (const decl of buffer.split(";")) {
          const i = decl.indexOf(":");
          const prop = decl.slice(0, i).trim();
          if (i > 0 && prop.startsWith("--")) result.set(prop, decl.slice(i + 1).trim());
        }
      }
      buffer = "";
    } else {
      buffer += ch;
    }
  }
  return result;
}

function squash(s: string): string {
  return s.replace(/\s+/g, "").replace(/["']/g, "").toLowerCase();
}

/** Canonical form of a CSS value so readable and minified output compare equal. */
export function normaliseValue(v: string): string {
  return v
    .trim()
    .toLowerCase()
    .replace(/["']/g, "")
    .replace(/\s*,\s*/g, ",")
    .replace(/\s+/g, " ")
    .replace(/#([0-9a-f])([0-9a-f])([0-9a-f])(?![0-9a-f])/g, "#$1$1$2$2$3$3")
    .replace(/(?<![#\w.])(-?(?:\d+\.?\d*|\.\d+))(ms|s|px|rem|em|%)?(?![\w.%])/g, (_m, num: string, unit = "") => {
      const n = unit === "s" ? Number(num) * 1000 : Number(num);
      const u = unit === "s" ? "ms" : unit;
      return n === 0 && u !== "%" ? "0" : `${n}${u}`;
    });
}
