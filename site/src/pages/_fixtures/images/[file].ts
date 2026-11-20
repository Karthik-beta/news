// Test images for the media components' fixture pages, served from the site's own origin (no third-party
// requests) and only in fixture builds (NEWS_FIXTURES=1), so they never reach a production dist/.
import type { APIRoute } from "astro";
import { fixturesEnabled } from "../../../renderer/fixtures/routes";

const images = import.meta.glob<string>("../../../renderer/fixtures/images/*.svg", {
  query: "?raw",
  import: "default",
  eager: true,
});

export function getStaticPaths() {
  if (!fixturesEnabled()) return [];
  return Object.entries(images).map(([path, body]) => ({
    params: { file: path.split("/").pop() as string },
    props: { body },
  }));
}

export const GET: APIRoute = ({ props }) =>
  new Response((props as { body: string }).body, { headers: { "Content-Type": "image/svg+xml; charset=utf-8" } });
