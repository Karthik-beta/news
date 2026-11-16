// The rest of each component fixture's bundle, next to its page: index.md and charts/<id>.csv and .json, so the
// download links resolve and the files can be checked against the data tables. NEWS_FIXTURES=1 builds only.
import type { APIRoute } from "astro";
import { componentFixtureBundle, componentFixturePaths } from "../../../../../renderer/fixtures/components.vite";
import { fixtureLibrary } from "../../../../../renderer/fixtures/library";
import { renderPage } from "../../../../../renderer/render";

const TYPES: Record<string, string> = {
  md: "text/markdown; charset=utf-8",
  csv: "text/csv; charset=utf-8",
  json: "application/json; charset=utf-8",
};

export function getStaticPaths() {
  return componentFixturePaths().flatMap(({ params, props }) => {
    const { files } = renderPage(componentFixtureBundle(props.fixture), { library: fixtureLibrary() });
    return Object.entries(files)
      .filter(([path]) => path !== "index.html")
      .map(([file, body]) => ({ params: { ...params, file }, props: { body } }));
  });
}

export const GET: APIRoute = ({ params, props }) =>
  new Response((props as { body: string }).body, {
    headers: { "Content-Type": TYPES[String(params.file).split(".").pop() ?? ""] ?? "text/plain" },
  });
