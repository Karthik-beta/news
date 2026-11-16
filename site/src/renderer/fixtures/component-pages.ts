// Component fixture pages: every src/components/<Name>/fixtures/<fixture>.oui rendered by the real renderer, with
// the data of every fixture bundle plus component-pages/ (quotes, a test study), the fixture glossary and the
// fixture images. Built only with NEWS_FIXTURES=1, at /fixtures/components/<Name>/<fixture>/. Vite only
// (import.meta.glob); the component tests and the fixture routes use it.
import { type PageBundleInput, type RenderResult, renderPage } from "../render";
import { mergeBundleData } from "./bundles/assemble";
import { fixtureBundles } from "./bundles/vite";
import componentData from "./component-pages/data.json";
import componentSources from "./component-pages/sources.json";
import { fixtureGlossary } from "./glossary";
import { fixtureLibrary } from "./library";
import { fixturesEnabled } from "./routes";

const files = import.meta.glob<string>("../../components/*/fixtures/*.oui", {
  query: "?raw",
  import: "default",
  eager: true,
});

export interface ComponentFixture {
  component: string;
  fixture: string;
  oui: string;
}

export const componentFixtures: ComponentFixture[] = Object.entries(files)
  .map(([path, oui]) => {
    const [, component, fixture] = /components\/([^/]+)\/fixtures\/([^/]+)\.oui$/.exec(path) ?? [];
    return { component: component as string, fixture: fixture as string, oui };
  })
  .sort((a, b) => `${a.component}/${a.fixture}`.localeCompare(`${b.component}/${b.fixture}`));

/** The data every component fixture page resolves against. */
export const componentFixtureData = (): Pick<PageBundleInput, "data" | "sources"> =>
  mergeBundleData(fixtureBundles["apple-fy2024"], fixtureBundles["edge-cases"], {
    oui: "",
    head: { flyTitle: "", headline: "", summary: "" },
    data: componentData,
    sources: componentSources,
  });

export function componentFixture(component: string, fixture: string): ComponentFixture {
  const found = componentFixtures.find((f) => f.component === component && f.fixture === fixture);
  if (!found) throw new Error(`No fixture ${component}/fixtures/${fixture}.oui`);
  return found;
}

/** Renders one component fixture through the real renderer (not strict: fixtures show the missing states). */
export function renderComponentFixture(component: string, fixture: string): RenderResult {
  const { oui } = componentFixture(component, fixture);
  return renderPage(
    {
      oui,
      ...componentFixtureData(),
      head: {
        flyTitle: "Component fixture",
        headline: `${component}: ${fixture}`,
        summary: `The ${component} component's "${fixture}" fixture, rendered by the page renderer.`,
      },
    },
    { library: fixtureLibrary(), glossary: fixtureGlossary },
  );
}

/** Paths for /fixtures/components/<name>/<fixture>/. */
export function componentFixturePaths(env?: Record<string, string | undefined>) {
  return fixturesEnabled(env)
    ? componentFixtures.map(({ component, fixture }) => ({ params: { name: component, fixture } }))
    : [];
}
