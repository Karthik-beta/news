// The other fixture curations, each served by its own `wrangler dev` of the built Worker (playwright.config.ts) on the
// ports after E2E_PORT. The main server serves the `normal` set.
export const FIXTURE_SETS = ["withdrawn-lead", "region", "empty-latest"] as const;
export type FixtureSet = (typeof FIXTURE_SETS)[number];

export const setPort = (set: FixtureSet): number =>
  Number(process.env.E2E_PORT ?? 4329) + 1 + FIXTURE_SETS.indexOf(set);
export const setOrigin = (set: FixtureSet): string => `http://127.0.0.1:${setPort(set)}`;
