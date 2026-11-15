// The rest of each renderer fixture's bundle, next to its page: index.md and charts/<id>.csv and .json, so links
// from the page resolve and the files can be inspected. NEWS_FIXTURES=1 builds only.
import type { APIRoute } from "astro";
import { fixtureBundles } from "../../../../renderer/fixtures/bundles/vite";
import { fixtureLibrary } from "../../../../renderer/fixtures/library";
import { rendererFixturePaths } from "../../../../renderer/fixtures/routes";
import { renderPage } from "../../../../renderer/render";

const TYPES: Record<string, string> = {
  md: "text/markdown; charset=utf-8",
  csv: "text/csv; charset=utf-8",
  json: "application/json; charset=utf-8",
};

export function getStaticPaths() {
  return rendererFixturePaths().flatMap(({ params }) => {
    const { files } = renderPage(fixtureBundles[params.bundle], { library: fixtureLibrary() });
    return Object.entries(files)
      .filter(([path]) => path !== "index.html")
      .map(([file, body]) => ({ params: { bundle: params.bundle, file }, props: { body } }));
  });
}

export const GET: APIRoute = ({ params, props }) =>
  new Response((props as { body: string }).body, {
    headers: { "Content-Type": TYPES[String(params.file).split(".").pop() ?? ""] ?? "text/plain" },
  });
