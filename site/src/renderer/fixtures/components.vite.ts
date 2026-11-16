// Component fixtures for code that runs through Vite (the fixture pages in the Astro build, Vitest).
import { fixtureBundles } from "./bundles/vite";
import { type ComponentFixture, componentFixtureInput, componentFixturesFrom } from "./components";
import { fixturesEnabled } from "./routes";

const files = import.meta.glob<string>("../../components/*/fixtures/*.oui", {
  query: "?raw",
  import: "default",
  eager: true,
});

export const componentFixtures: ComponentFixture[] = componentFixturesFrom(files);

export const componentFixtureBundle = (f: ComponentFixture) => componentFixtureInput(f, fixtureBundles);

/** Paths for /fixtures/components/<name>/<fixture>/, only when fixture pages are built. */
export function componentFixturePaths(env?: Record<string, string | undefined>) {
  return fixturesEnabled(env)
    ? componentFixtures.map((f) => ({ params: { name: f.name, fixture: f.fixture }, props: { fixture: f } }))
    : [];
}
