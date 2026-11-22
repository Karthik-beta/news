// The reader-facing static pages' copy (content/pages/*.md): read at build time, with every {token} replaced by a
// value from src/config/site.ts. Copy never types a configured value, and a token with no value fails the build.
import { type SiteConfig, siteConfig } from "../config/site";
import { type Block, parseBlocks, plainText, stripComments } from "./markdown";

const files = import.meta.glob("../../content/pages/*.md", { query: "?raw", import: "default", eager: true }) as Record<
  string,
  string
>;

/** The copy file of each page, by file name without `.md`. */
export const rawCopy = (): Record<string, string> =>
  Object.fromEntries(Object.entries(files).map(([path, md]) => [path.replace(/^.*\/(.+)\.md$/, "$1"), md]));

const TOKEN = /\{([A-Za-z]+(?:\.[A-Za-z]+)*)\}/g;

/** Replaces {dotted.config.keys}. Comments go first: the author's notes may name keys that do not exist. */
export function fillTokens(md: string, config: SiteConfig = siteConfig): string {
  return stripComments(md).replace(TOKEN, (_m, path: string) => {
    const value = path.split(".").reduce<unknown>((o, k) => (o as Record<string, unknown> | undefined)?.[k], config);
    if (typeof value !== "string" && typeof value !== "number")
      throw new Error(`Copy token {${path}} has no value in site/src/config/site.ts`);
    return String(value);
  });
}

export interface StaticDoc {
  title: string;
  description: string;
  /** ISO date of the last edit, from the copy's front matter. */
  updated: string;
  blocks: Block[];
}

export function parseDoc(raw: string, config: SiteConfig = siteConfig): StaticDoc {
  const m = /^---\n([\s\S]*?)\n---\n([\s\S]*)$/.exec(raw.replace(/\r\n?/g, "\n"));
  if (!m) throw new Error("Copy file has no front matter");
  const meta = Object.fromEntries(
    (m[1] as string).split("\n").map((l) => [l.slice(0, l.indexOf(":")).trim(), l.slice(l.indexOf(":") + 1).trim()]),
  );
  for (const k of ["title", "description", "updated"]) if (!meta[k]) throw new Error(`Copy front matter lacks ${k}`);
  return {
    title: meta.title as string,
    description: meta.description as string,
    updated: meta.updated as string,
    blocks: parseBlocks(fillTokens(m[2] as string, config)),
  };
}

/** The copy of one page. `name` is the file name without `.md`, a source file of content/pages/. */
export function loadDoc(name: string): StaticDoc {
  const raw = rawCopy()[name];
  if (raw === undefined) throw new Error(`No copy file content/pages/${name}.md`);
  return parseDoc(raw);
}

/** The blocks without the section headed `heading` (up to the next h2). */
export function withoutSection(blocks: readonly Block[], heading: string): Block[] {
  let skipping = false;
  return blocks.filter((b) => {
    if (b.type === "h2") skipping = b.text === heading;
    return !skipping;
  });
}

export interface GlossaryItem {
  term: string;
  slug: string;
  /** The definition as Markdown (inline). */
  definition: string;
  letter: string;
}

const entrySlug = (term: string) =>
  term
    .toLowerCase()
    .normalize("NFKD")
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");

/** The A-Z entries of glossary.md: paragraphs `**Term.** Definition` under one-letter headings. */
export function glossaryItems(doc: StaticDoc = loadDoc("glossary")): GlossaryItem[] {
  let letter = "";
  const out: GlossaryItem[] = [];
  for (const b of doc.blocks) {
    if (b.type === "h2") letter = /^[A-Z]$/.test(b.text) ? b.text : "";
    else if (b.type === "p" && letter) {
      const m = /^\*\*(.+?)\*\*\s*(.*)$/.exec(b.text);
      if (!m) throw new Error(`Glossary entry under ${letter} does not start with a bold term: ${b.text.slice(0, 40)}`);
      const term = plainText(m[1] as string).replace(/\.$/, "");
      if (!term.toUpperCase().startsWith(letter))
        throw new Error(`Glossary term "${term}" is under the wrong letter ${letter}`);
      out.push({ term, slug: entrySlug(term), definition: m[2] as string, letter });
    }
  }
  return out;
}
