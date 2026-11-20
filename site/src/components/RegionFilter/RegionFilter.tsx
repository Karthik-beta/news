// The region filter (design/components.md, "Date line and region filter"): region links, the selected one in ink,
// weight 600, with a 2px crimson underline; the others in secondary ink. Plain links, so it works with no
// JavaScript: each region is its own URL. A page-template part (home in the date line; hubs under the hub head).
export interface Region {
  slug: string;
  label: string;
}

/** The regions of the PRD, in their fixed order. */
export const REGIONS: readonly Region[] = [
  { slug: "all", label: "All regions" },
  { slug: "north-america", label: "North America" },
  { slug: "europe", label: "Europe" },
  { slug: "india", label: "India" },
  { slug: "china", label: "China" },
  { slug: "asia", label: "Asia" },
  { slug: "australia", label: "Australia" },
  { slug: "global", label: "Global" },
];

/** "/" for all regions, "/?region=india" for one. */
export const regionHref = (base: string, slug: string) =>
  slug === "all" ? base : `${base}?region=${encodeURIComponent(slug)}`;

export interface RegionFilterProps {
  selected: string;
  regions?: readonly Region[];
  /** The page the filter applies to. */
  base?: string;
  /** The navigation landmark's name. Change it only where two filters share a page (fixture galleries). */
  label?: string;
}

export function RegionFilter({ selected, regions = REGIONS, base = "/", label = "Regions" }: RegionFilterProps) {
  return (
    <nav className="region-filter" aria-label={label}>
      <ul className="region-filter__list">
        {regions.map((r) => (
          <li key={r.slug}>
            <a
              className="region-filter__link"
              href={regionHref(base, r.slug)}
              aria-current={r.slug === selected ? "page" : undefined}
            >
              {r.label}
            </a>
          </li>
        ))}
      </ul>
    </nav>
  );
}
