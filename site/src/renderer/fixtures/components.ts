// Component fixture pages: each src/components/<Name>/fixtures/<fixture>.oui rendered by the real renderer with the
// data of every fixture bundle merged (apple-fy2024, edge-cases, charts), at /fixtures/components/<Name>/<fixture>/
// (NEWS_FIXTURES=1 builds only). The same inputs feed the component tests, so the browser and the unit tests see
// the same pages.
import type { PageBundleInput } from "../render";
import { type FixtureBundleName, mergeBundleData } from "./bundles/assemble";

export interface ComponentFixture {
  /** The component folder ("ColumnChart"). */
  name: string;
  /** The fixture file's name without .oui ("edge-cases"). */
  fixture: string;
  oui: string;
}

/** Fixtures from a map of `<Name>/fixtures/<fixture>.oui` paths (any prefix) to their text, in name order. */
export function componentFixturesFrom(files: Record<string, string>): ComponentFixture[] {
  return Object.entries(files)
    .flatMap(([path, oui]) => {
      const m = /([A-Z][A-Za-z0-9]*)\/fixtures\/([a-z0-9-]+)\.oui$/.exec(path.replaceAll("\\", "/"));
      return m ? [{ name: m[1] as string, fixture: m[2] as string, oui }] : [];
    })
    .sort((a, b) => `${a.name}/${a.fixture}`.localeCompare(`${b.name}/${b.fixture}`));
}

/** The renderer input for a component fixture: its page.oui with every fixture bundle's data and sources. */
export function componentFixtureInput(
  f: ComponentFixture,
  bundles: Record<FixtureBundleName, PageBundleInput>,
): PageBundleInput {
  const merged = mergeBundleData(bundles["apple-fy2024"], bundles["edge-cases"], bundles.charts);
  return {
    oui: f.oui,
    data: merged.data,
    sources: merged.sources,
    head: {
      flyTitle: "Component fixture",
      headline: `${f.name}: ${f.fixture}`,
      summary: `Every case in ${f.name}/fixtures/${f.fixture}.oui, rendered by the page renderer.`,
    },
  };
}
