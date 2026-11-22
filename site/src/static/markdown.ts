// The small Markdown the static-page copy (content/pages/*.md) is written in, rendered to HTML on the server.
// Only what the copy uses: ## and ### headings, paragraphs, - and 1. lists, pipe tables, **bold**, `code` and
// [links](...). HTML comments (the author's TODO notes) are dropped. Everything else prints as typed, escaped.
// A link goes only to a site path, https:, mailto: or a glossary term ([words](glossary:slug)).

export type Block =
  | { type: "h2" | "h3"; text: string; id: string }
  | { type: "p"; text: string }
  | { type: "ul" | "ol"; items: string[] }
  | { type: "table"; head: string[]; rows: string[][] };

export const stripComments = (md: string) => md.replace(/<!--[\s\S]*?-->/g, "");

const slug = (text: string) =>
  plainText(text)
    .toLowerCase()
    .normalize("NFKD")
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "") || "section";

const cells = (line: string) =>
  line
    .trim()
    .replace(/^\||\|$/g, "")
    .split("|")
    .map((c) => c.trim());

/** Blocks of a Markdown body, in order. Heading ids are unique on the page. */
export function parseBlocks(md: string): Block[] {
  const lines = stripComments(md).replace(/\r\n?/g, "\n").split("\n");
  const blocks: Block[] = [];
  const used = new Set<string>();
  const uniqueId = (text: string) => {
    const base = slug(text);
    let id = base;
    for (let n = 2; used.has(id); n++) id = `${base}-${n}`;
    used.add(id);
    return id;
  };
  for (let i = 0; i < lines.length; ) {
    const line = (lines[i] as string).trim();
    const heading = /^(#{2,3})\s+(.+)$/.exec(line);
    if (line === "") {
      i++;
    } else if (heading) {
      const text = (heading[2] as string).trim();
      blocks.push({ type: heading[1] === "##" ? "h2" : "h3", text, id: uniqueId(text) });
      i++;
    } else if (/^(-|\d+\.)\s+/.test(line)) {
      const ordered = /^\d+\./.test(line);
      const item = ordered ? /^\d+\.\s+/ : /^-\s+/;
      const items: string[] = [];
      while (i < lines.length && item.test((lines[i] as string).trim())) {
        items.push((lines[i] as string).trim().replace(item, ""));
        i++;
      }
      blocks.push({ type: ordered ? "ol" : "ul", items });
    } else if (line.startsWith("|")) {
      const rows: string[][] = [];
      while (i < lines.length && (lines[i] as string).trim().startsWith("|")) {
        rows.push(cells(lines[i] as string));
        i++;
      }
      const [head = [], , ...body] = rows; // the second row is the --- separator
      blocks.push({ type: "table", head, rows: body });
    } else {
      const text: string[] = [];
      while (i < lines.length) {
        const l = (lines[i] as string).trim();
        if (l === "" || /^(#{2,3}\s|-\s|\d+\.\s|\|)/.test(l)) break;
        text.push(l);
        i++;
      }
      blocks.push({ type: "p", text: text.join(" ") });
    }
  }
  return blocks;
}

export const escapeHtml = (s: string) =>
  s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

const INLINE = /`([^`]+)`|\[([^\]]+)\]\(([^)\s]+)\)|\*\*(.+?)\*\*/g;
const SAFE_HREF = /^(\/(?!\/)|https:\/\/|mailto:)/;

export interface InlineOptions {
  /** Renders a glossary term ([words](glossary:slug)); without one the words stay plain. */
  term?: (words: string, slug: string) => string;
}

/** Inline Markdown as HTML. */
export function inlineHtml(text: string, options: InlineOptions = {}): string {
  let out = "";
  let last = 0;
  for (const m of text.matchAll(INLINE)) {
    out += escapeHtml(text.slice(last, m.index));
    last = m.index + m[0].length;
    const [, code, label, href, bold] = m;
    if (code !== undefined) out += `<code>${escapeHtml(code)}</code>`;
    else if (bold !== undefined) out += `<strong>${inlineHtml(bold, options)}</strong>`;
    else if (href?.startsWith("glossary:"))
      out += options.term?.(plainText(label as string), href.slice(9)) ?? escapeHtml(plainText(label as string));
    else if (href && SAFE_HREF.test(href))
      out += `<a href="${escapeHtml(href)}">${inlineHtml(label as string, { ...options, term: undefined })}</a>`;
    else out += escapeHtml(label as string);
  }
  return out + escapeHtml(text.slice(last));
}

/** Inline Markdown as plain words: links and emphasis dropped. */
export function plainText(text: string): string {
  return text.replace(
    INLINE,
    (_m, code, label, _href, bold) => code ?? (label !== undefined ? plainText(label) : plainText(bold)),
  );
}

/** The blocks as HTML, with the classes the Prose component's styles use (components/Prose/Prose.css). */
export function renderBlocks(blocks: readonly Block[], options: InlineOptions = {}): string {
  const inline = (t: string) => inlineHtml(t, options);
  return blocks
    .map((b) => {
      switch (b.type) {
        case "h2":
          return `<h2 class="prose__heading text-headline-l" id="${b.id}">${inline(b.text)}</h2>`;
        case "h3":
          return `<h3 class="prose__subheading text-headline-s" id="${b.id}">${inline(b.text)}</h3>`;
        case "p":
          return `<p>${inline(b.text)}</p>`;
        case "ul":
        case "ol":
          return `<${b.type} class="prose__list">${b.items.map((i) => `<li>${inline(i)}</li>`).join("")}</${b.type}>`;
        default: // table
          return `<table class="static-table"><thead><tr>${b.head.map((c) => `<th scope="col">${inline(c)}</th>`).join("")}</tr></thead><tbody>${b.rows
            .map(
              (r) =>
                `<tr>${r.map((c, n) => (n === 0 ? `<th scope="row">${inline(c)}</th>` : `<td>${inline(c)}</td>`)).join("")}</tr>`,
            )
            .join("")}</tbody></table>`;
      }
    })
    .join("\n");
}
