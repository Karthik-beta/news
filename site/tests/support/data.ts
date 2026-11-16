// Synthetic page data for component tests: build a resolver from a few data points without a fixture bundle, so a
// test can pin one currency, scale, unit, period or locator at a time. Everything goes through the real schema
// (PageData.parse / PageSources.parse), so a test can't build data the renderer would reject.
import { PageData, PageSources, type SourceT } from "../../src/renderer/bundle/schema";
import { PageDataResolver } from "../../src/renderer/data/resolve";
import { mergeBundleData } from "../../src/renderer/fixtures/bundles/assemble";
import { fixtureBundles } from "../../src/renderer/fixtures/bundles/vite";

export const TEST_SOURCE: SourceT = {
  title: "Test statistics office, annual release",
  short: "Annual release",
  publisher: "Test statistics office",
  connector: "test",
  type: "dataset",
  primary: true,
  url: "https://example.org/release",
  licence: { name: "CC BY 4.0", redistribution: true },
  retrieved_at: "2024-11-01",
};

export interface PointInput {
  value: string | null;
  unit?: string;
  currency?: string | null;
  scale?: "units" | "thousands" | "millions" | "billions" | "trillions" | null;
  decimals?: number | null;
  label?: string;
  period?: string;
  source?: string;
  locator?: Record<string, unknown>;
}

export interface SyntheticData {
  points?: Record<string, PointInput>;
  derived?: Record<string, Record<string, unknown>>;
  periods?: Record<string, { label: string; fiscal?: boolean }>;
  sources?: Record<string, SourceT>;
}

export function syntheticResolver({ points = {}, derived = {}, periods = {}, sources = {} }: SyntheticData) {
  const data = PageData.parse({
    version: 1,
    points: Object.fromEntries(
      Object.entries(points).map(([ref, p]) => [
        ref,
        {
          label: p.label ?? "Test value",
          value: p.value,
          value_text: p.value,
          unit: p.unit ?? p.currency ?? "Number",
          currency: p.currency ?? null,
          scale: p.scale ?? null,
          ...(p.decimals === undefined ? {} : { decimals: p.decimals }),
          period: p.period ?? "pd_test_2024",
          source: p.source ?? "src_test",
          locator: p.locator ?? { kind: "json", path: "$.value", series: "TEST" },
        },
      ]),
    ),
    derived,
    series: {},
    periods: { pd_test_2024: { label: "2024" }, ...periods },
    quotes: {},
    context: {},
  });
  const pageSources = PageSources.parse({ version: 1, sources: { src_test: TEST_SOURCE, ...sources } });
  return new PageDataResolver(data, pageSources);
}

/** The data every primitive gallery page renders with: the Apple and edge-case fixture bundles merged. */
export function fixtureResolver() {
  const merged = mergeBundleData(fixtureBundles["apple-fy2024"], fixtureBundles["edge-cases"]);
  return new PageDataResolver(PageData.parse(merged.data), PageSources.parse(merged.sources));
}
