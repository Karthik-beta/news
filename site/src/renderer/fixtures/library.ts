// The library fixture pages and renderer tests use: every production component plus the fixture-only components
// in ./components (a text block, a key figure and a line chart island). Production never includes these.
import type { AnyComponentSpec } from "../../components/contract";
import { componentSpecs, createNewsLibrary, type NewsLibrary } from "../../components/library";
import { specs } from "../../components/registry.generated";
import fixtureFigure from "./components/FixtureFigure/spec";
import fixtureLineChart from "./components/FixtureLineChart/spec";
import fixtureText from "./components/FixtureText/spec";

export const fixtureSpecs: AnyComponentSpec[] = [fixtureText, fixtureFigure, fixtureLineChart] as AnyComponentSpec[];

let cached: NewsLibrary | undefined;
export function fixtureLibrary(): NewsLibrary {
  cached ??= createNewsLibrary([...componentSpecs(specs), ...fixtureSpecs]);
  return cached;
}
