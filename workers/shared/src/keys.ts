// Key builders for the single table (ADR 0004). The only place key strings are put together.
import type { Key } from "./dynamo/table";

const VERSION_DIGITS = 6;

/** 'V#000042'. Zero-padded so that sort keys order versions numerically. */
export function versionSk(version: number): string {
  if (!Number.isInteger(version) || version < 1 || version >= 10 ** VERSION_DIGITS)
    throw new Error(`version out of range: ${version}`);
  return `V#${String(version).padStart(VERSION_DIGITS, "0")}`;
}

export function versionFromSk(sk: string): number {
  const m = /^V#([0-9]{6})$/.exec(sk);
  if (!m?.[1]) throw new Error(`not a version sort key: ${sk}`);
  return Number(m[1]);
}

export const LATEST = "LATEST";

export const pagePk = (pageId: string) => `PAGE#${pageId}`;
export const pageLatestKey = (pageId: string): Key => ({ PK: pagePk(pageId), SK: LATEST });
export const pageVersionKey = (pageId: string, version: number): Key => ({
  PK: pagePk(pageId),
  SK: versionSk(version),
});

export const hubPk = (hub: string) => `HUB#${hub}`;
/** Hub sort key: the page's first publication time, then its ID; an update keeps the page's place in the hub. */
export const hubSk = (publishedAt: string, pageId: string) => `${publishedAt}#${pageId}`;

export const entityKey = (entityId: string): Key => ({ PK: `ENT#${entityId}`, SK: "META" });

export const TAXONOMY_PK = "TAX#tree";
export const taxonomyNodeKey = (id: string): Key => ({ PK: TAXONOMY_PK, SK: `NODE#${id}` });

export const redirectKey = (path: string): Key => ({ PK: `URL#${path}`, SK: "REDIRECT" });
export const pathClaimKey = (path: string): Key => ({ PK: `URL#${path}`, SK: "PAGE" });

export const CURATION_PK = "HOME#CURATION";
export const curationLatestKey = (): Key => ({ PK: CURATION_PK, SK: LATEST });
export const curationVersionKey = (version: number): Key => ({ PK: CURATION_PK, SK: versionSk(version) });

export const idempotencyKey = (keyHash: string): Key => ({ PK: `IDEM#${keyHash}`, SK: "META" });

/** Reports are partitioned by month of receipt: 'REPORT#2026-11'. */
export const reportPk = (createdAt: string) => `REPORT#${createdAt.slice(0, 7)}`;
export const reportKey = (createdAt: string, reportId: string): Key => ({
  PK: reportPk(createdAt),
  SK: `${createdAt}#${reportId}`,
});

/** Hub IDs: 'latest', 'cat/<slug>', 'ent/<entity id>', 'region/<region>'. */
export const HUB_LATEST = "latest";
export const categoryHub = (slug: string) => `cat/${slug}`;
export const entityHub = (entityId: string) => `ent/${entityId}`;
export const regionHub = (region: string) => `region/${region}`;

/** Every ancestor and the slug itself: 'economy/inflation/core' → economy, economy/inflation, economy/inflation/core. */
export function categoryLineage(slug: string): string[] {
  const parts = slug.split("/");
  return parts.map((_, i) => parts.slice(0, i + 1).join("/"));
}

/** The hubs a page is listed in: latest, each category and its parents, each entity, its region. */
export function hubsForPage(meta: { categories: string[]; entities: { id: string }[]; region: string }): string[] {
  const hubs = new Set<string>([HUB_LATEST]);
  for (const c of meta.categories) for (const slug of categoryLineage(c)) hubs.add(categoryHub(slug));
  for (const e of meta.entities) hubs.add(entityHub(e.id));
  hubs.add(regionHub(meta.region));
  return [...hubs];
}
