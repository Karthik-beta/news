// Public URLs the Workers purge or link to. The SSR site (#14) must serve the same paths; ADR 0005 lists them.

export const DEFAULT_SITE_ORIGIN = "https://news.karthik.nz";

/** Region views of the home page (Bureau Design 4.2 coverage regions). Overridable with NEWS_REGION_VIEWS. */
export const DEFAULT_REGION_VIEWS = [
  "north-america",
  "europe",
  "india",
  "china",
  "asia",
  "australia",
  "global",
] as const;

const REGION = /^[a-z]+(?:-[a-z]+)*$/;

/** Parses 'a,b,c'; falls back to the default list when empty. Invalid codes are dropped. */
export function regionViews(setting?: string): string[] {
  const list = (setting ?? "")
    .split(",")
    .map((s) => s.trim())
    .filter((s) => REGION.test(s));
  return list.length > 0 ? [...new Set(list)] : [...DEFAULT_REGION_VIEWS];
}

export const HOME_PATH = "/";
export const regionViewPath = (region: string) => `/region/${region}`;

/** Path of a hub page: latest → /latest, cat/<slug> → /<slug>, ent/<id> → /entity/<id>, region/<r> → /region/<r>. */
export function hubPath(hub: string): string {
  if (hub === "latest") return "/latest";
  if (hub.startsWith("cat/")) return `/${hub.slice(4)}`;
  if (hub.startsWith("ent/")) return `/entity/${encodeURIComponent(hub.slice(4))}`;
  if (hub.startsWith("region/")) return regionViewPath(hub.slice(7));
  throw new Error(`unknown hub: ${hub}`);
}

/** The URLs of a page's files on the CDN (R2 served under the page path). */
export function pageFileUrls(origin: string, path: string, files: string[]): string[] {
  const urls = [`${origin}${path}`, `${origin}${path}/`];
  for (const f of files) if (f !== "index.html") urls.push(`${origin}${path}/${f}`);
  return urls;
}

/** Home and every region view: exactly what a curation change purges. */
export function homeUrls(origin: string, regions: string[]): string[] {
  return [`${origin}${HOME_PATH}`, ...regions.map((r) => `${origin}${regionViewPath(r)}`)];
}

export function absolute(origin: string, path: string): string {
  return `${origin}${path}`;
}
