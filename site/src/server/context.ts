// Wires a request to the data layer: settings from the Worker's env, the data adapter, and the isolate's taxonomy cache.
import type { Reader } from "@news/worker-shared/read";
import type { TaxonomyNode } from "@news/worker-shared/records";
import { DEFAULT_SITE_ORIGIN, regionViews } from "@news/worker-shared/urls";
import { type SiteEnv, siteData } from "../data";
import type { Ctx } from "./views";

/** The category tree is read at most once a minute per isolate: every page needs it for the navigation and the 404 check. */
export class TaxonomyCache {
  #value: { at: number; nodes: TaxonomyNode[] } | undefined;
  #pending: Promise<TaxonomyNode[]> | undefined;

  constructor(
    private readonly ttlMs = 60_000,
    private readonly clock: () => number = Date.now,
  ) {}

  async get(reader: Reader): Promise<TaxonomyNode[]> {
    if (this.#value && this.clock() - this.#value.at < this.ttlMs) return this.#value.nodes;
    this.#pending ??= reader
      .listTaxonomy()
      .then((nodes) => {
        this.#value = { at: this.clock(), nodes };
        return nodes;
      })
      .finally(() => {
        this.#pending = undefined;
      });
    return this.#pending;
  }

  clear(): void {
    this.#value = undefined;
  }
}

export const taxonomyCache = new TaxonomyCache();

/** The site's origin (no trailing slash) and region views, from the env: the same values the api reads to purge. */
export function settings(env: Pick<SiteEnv, "NEWS_SITE_ORIGIN" | "NEWS_REGION_VIEWS">) {
  return {
    origin: (env.NEWS_SITE_ORIGIN ?? DEFAULT_SITE_ORIGIN).replace(/\/+$/, ""),
    regions: regionViews(env.NEWS_REGION_VIEWS),
  };
}

export async function siteContext(env: SiteEnv, now: Date = new Date()): Promise<Ctx & { origin: string }> {
  const data = await siteData(env);
  const { origin, regions } = settings(env);
  return { data, origin, regions, now, taxonomy: () => taxonomyCache.get(data.reader) };
}
