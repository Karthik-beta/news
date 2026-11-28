// The rest of each fixture page's bundle, next to its page: index.md and charts/<id>.csv and .json, so links from the
// page resolve and the files can be inspected. Rendered from the page's committed inputs with the production library,
// as Plate renders them; chart JSON is not a bundle file (manifest.v1), so it exists only here. NEWS_FIXTURES=1 only.
import type { APIRoute } from "astro";
import { fixturesEnabled } from "../../../../renderer/fixtures/routes";
import { renderPage } from "../../../../renderer/render";

const TYPES: Record<string, string> = {
  md: "text/markdown; charset=utf-8",
  csv: "text/csv; charset=utf-8",
  json: "application/json; charset=utf-8",
};

const inputs = import.meta.glob<string>(
  [
    "../../../../../../pipeline/fixtures/pages/*/page.oui",
    "../../../../../../pipeline/fixtures/pages/*/{data,sources,copy}.json",
  ],
  { query: "?raw", import: "default", eager: true },
);

function pageFiles(): Record<string, Record<string, string>> {
  const raw: Record<string, Record<string, string>> = {};
  for (const [path, text] of Object.entries(inputs)) {
    const [, page = "", file = ""] = /pages\/([^/]+)\/([^/]+)$/.exec(path) ?? [];
    const entry = raw[page] ?? {};
    entry[file] = text;
    raw[page] = entry;
  }
  return Object.fromEntries(
    Object.entries(raw).map(([page, f]) => {
      const copy = JSON.parse(f["copy.json"] as string).copy;
      const { files } = renderPage({
        oui: f["page.oui"] as string,
        data: JSON.parse(f["data.json"] as string),
        sources: JSON.parse(f["sources.json"] as string),
        head: { flyTitle: copy.fly_title, headline: copy.headline, summary: copy.summary },
      });
      return [page, files];
    }),
  );
}

export function getStaticPaths() {
  if (!fixturesEnabled()) return [];
  return Object.entries(pageFiles()).flatMap(([page, files]) =>
    Object.entries(files)
      .filter(([file]) => file !== "index.html")
      .map(([file, body]) => ({ params: { page, file }, props: { body } })),
  );
}

export const GET: APIRoute = ({ params, props }) =>
  new Response((props as { body: string }).body, {
    headers: { "Content-Type": TYPES[String(params.file).split(".").pop() ?? ""] ?? "text/plain" },
  });
