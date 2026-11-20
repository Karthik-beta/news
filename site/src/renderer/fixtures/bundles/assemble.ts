// Turns the raw files of a fixture bundle folder into a renderer input. Shared by the Vite loader (fixture pages,
// Vitest) and the file-system loader (Bun scripts and SSR tests). A folder without data.json and sources.json
// (static/) uses apple-fy2024's.
import type { PageBundleInput, PageHead } from "../../render";

export interface RawBundle {
  oui: string;
  head: string;
  data?: string;
  sources?: string;
}

export const FIXTURE_BUNDLES = ["apple-fy2024", "edge-cases", "static", "charts"] as const;
export type FixtureBundleName = (typeof FIXTURE_BUNDLES)[number];
const DATA_FROM: Partial<Record<FixtureBundleName, FixtureBundleName>> = { static: "apple-fy2024" };

export function assembleBundles(raw: Record<string, RawBundle>): Record<FixtureBundleName, PageBundleInput> {
  const out = {} as Record<FixtureBundleName, PageBundleInput>;
  for (const name of FIXTURE_BUNDLES) {
    const files = raw[name];
    const dataFiles = raw[DATA_FROM[name] ?? name];
    if (!files?.oui || !files.head || !dataFiles?.data || !dataFiles.sources) {
      throw new Error(`Fixture bundle ${name} is incomplete`);
    }
    out[name] = {
      oui: files.oui,
      head: JSON.parse(files.head) as PageHead,
      data: JSON.parse(dataFiles.data),
      sources: JSON.parse(dataFiles.sources),
    };
  }
  return out;
}

type Json = Record<string, Record<string, unknown>>;

/** Data and sources of several bundles in one, for galleries that show values from all of them. */
export function mergeBundleData(...bundles: PageBundleInput[]): Pick<PageBundleInput, "data" | "sources"> {
  const data: Json = {};
  const sources: Json = {};
  for (const b of bundles) {
    for (const [k, v] of Object.entries(b.data as Json)) {
      data[k] = typeof v === "object" && v !== null ? { ...(data[k] ?? {}), ...v } : v;
    }
    for (const [k, v] of Object.entries(b.sources as Json)) {
      sources[k] = typeof v === "object" && v !== null ? { ...(sources[k] ?? {}), ...v } : v;
    }
  }
  return { data, sources };
}
