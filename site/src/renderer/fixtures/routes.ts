// Fixture pages are built only when NEWS_FIXTURES=1 (test and CI builds). Production builds leave them out, so
// they never ship. Every fixture route's getStaticPaths goes through `fixturesEnabled`.
import { FIXTURE_BUNDLES } from "./bundles/assemble";

export function fixturesEnabled(env: Record<string, string | undefined> = process.env): boolean {
  return env.NEWS_FIXTURES === "1";
}

/** Paths for the renderer fixture pages: /fixtures/renderer/<bundle>/. */
export function rendererFixturePaths(env?: Record<string, string | undefined>) {
  return fixturesEnabled(env) ? FIXTURE_BUNDLES.map((bundle) => ({ params: { bundle } })) : [];
}
