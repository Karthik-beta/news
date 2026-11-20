// Galleries of each primitive's fixture cases (src/components/<Name>/fixtures/cases.tsx), rendered on the server
// with the data of every fixture bundle, so SourceRef cases resolve and source links have targets.
import type { FixtureCase } from "../../components/contract";
import { PageData, PageSources } from "../bundle/schema";
import { PageDataProvider } from "../data/context";
import { PageDataResolver } from "../data/resolve";
import { staticMarkup } from "../markup";
import { SourcesList } from "../render";
import { mergeBundleData } from "./bundles/assemble";
import { fixtureBundles } from "./bundles/vite";

const modules = import.meta.glob<FixtureCase[]>("../../components/*/fixtures/cases.tsx", {
  import: "cases",
  eager: true,
});

const galleries: Record<string, FixtureCase[]> = Object.fromEntries(
  Object.entries(modules).map(([path, cases]) => [/components\/([^/]+)\//.exec(path)?.[1] ?? path, cases]),
);

export const galleryNames = () => Object.keys(galleries).sort();

export function renderGallery(name: string): string {
  const cases = galleries[name];
  if (!cases) throw new Error(`No fixture cases for ${name}`);
  const merged = mergeBundleData(fixtureBundles["apple-fy2024"], fixtureBundles["edge-cases"], fixtureBundles.charts);
  const resolver = new PageDataResolver(PageData.parse(merged.data), PageSources.parse(merged.sources));
  const allSources = Object.keys(resolver.sources.sources);
  return staticMarkup(
    <PageDataProvider resolver={resolver}>
      <article className="article fixture-gallery">
        <header className="article__head">
          <p className="text-fly">Primitive</p>
          <h1 className="text-headline-xl">{name}</h1>
        </header>
        {cases.map((c) => (
          <section key={c.name} className="fixture-case" aria-label={c.name}>
            <h2 className="fixture-case__name">{c.name}</h2>
            {c.render()}
          </section>
        ))}
        <SourcesList ids={allSources} resolver={resolver} />
      </article>
    </PageDataProvider>,
  );
}
