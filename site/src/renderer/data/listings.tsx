// Listings: the other pages a home, hub, search or rail list shows. List components take a page ID (and, on the
// home page, a figure or chart ID) and read everything they write from that page's own record: its fly title,
// headline, summary, type, region and dates, and its own data for a figure. Nothing is typed by a curator; a list
// has no prop for text (design/layout.md, "Curated stories use their page's own words").
//
// The template that renders a list (home, hubs, search: #14) reads the page records, hub entries or search hits,
// and each slot page's data.json, and passes them in through <ListingsProvider>.
import type { ElementNode } from "@openuidev/react-lang";
import { createContext, type ReactNode, useContext } from "react";
import { REGIONS } from "../../components/RegionFilter/RegionFilter";
import type { PageDataResolver } from "./resolve";

export type PageType = "earnings" | "macro" | "policy" | "report" | "news" | "science" | "explainer";

/**
 * A page's listing fields, named as the page record (PAGE#…/LATEST), its hub entries and search results store
 * them (contracts/cloud/dynamodb-items.schema.json, search-api.openapi.json). The text is final: the renderer
 * wrote it with every value filled in, so lists print it exactly as stored.
 */
export interface PageListing {
  page_id: string;
  /** The page's public path (or a search hit's URL). */
  path: string;
  title: string;
  /** Records may have none (search hits never do); the list then shows no fly title. */
  fly_title?: string | null;
  summary: string;
  page_type: PageType;
  /** Region code: "north-america", "india", "global". */
  region: string;
  /** UTC timestamps, RFC 3339. */
  published_at: string;
  updated_at?: string | null;
}

/** One of the page's key figures, as its own page.oui writes it: the figure's label and its trend series. */
export interface ListedFigure {
  label: string;
  trend?: string | null;
}

/** One of the page's charts, by chart ID (its statement name): the chart's own title and the series it is about. */
export interface ListedChart {
  title: string;
  series: string;
}

export interface ListedPage {
  record: PageListing;
  /** The page's own data.json and sources.json, for a slot's figure or mini chart. */
  data?: PageDataResolver;
  /** The page's key figures by value reference (dp_ or dv_), from its page.oui. */
  figures?: Readonly<Record<string, ListedFigure>>;
  /** The page's charts by chart ID, from its page.oui. */
  charts?: Readonly<Record<string, ListedChart>>;
}

/** The pages a list can show, by page ID. A page that is not here (withdrawn, not found) is left out of the list. */
export class PageListings {
  private readonly byId = new Map<string, ListedPage>();
  /** Page IDs a list asked for and did not find, so the template can back-fill the slot. */
  readonly missing = new Set<string>();

  constructor(pages: Iterable<ListedPage>) {
    for (const p of pages) this.byId.set(p.record.page_id, p);
  }

  get(pageId: string): ListedPage | null {
    const page = this.byId.get(pageId);
    if (!page) this.missing.add(pageId);
    return page ?? null;
  }
}

const ListingsContext = createContext<PageListings | null>(null);

export function ListingsProvider({ listings, children }: { listings: PageListings; children: ReactNode }) {
  return <ListingsContext.Provider value={listings}>{children}</ListingsContext.Provider>;
}

/** The listings the template passed in. List components call this to turn page IDs into records. */
export function useListings(): PageListings {
  const listings = useContext(ListingsContext);
  if (!listings) throw new Error("useListings() needs a <ListingsProvider> from the list's page template.");
  return listings;
}

/** The page types as readers see them in list meta lines. */
export const PAGE_TYPE_LABELS: Record<PageType, string> = {
  earnings: "Earnings",
  macro: "Economic data",
  policy: "Policy decision",
  report: "Report",
  news: "News",
  science: "Science",
  explainer: "Explainer",
};

/** Words a region's name keeps in lower case after its first word ("Middle East and North Africa"). */
const MINOR_WORDS = new Set(["and", "of", "the"]);

/**
 * "North America" for "north-america": the region filter's label, or, for a region it lacks, the code's words as a
 * place name, each capitalised but "and", "of" and "the" ("latin-america" is "Latin America", not "Latin america").
 */
export function regionLabel(code: string): string {
  const known = REGIONS.find((r) => r.slug === code);
  if (known) return known.label;
  return code
    .split("-")
    .filter((w) => w.length > 0)
    .map((w, i) => (i > 0 && MINOR_WORDS.has(w) ? w : w.charAt(0).toUpperCase() + w.slice(1)))
    .join(" ");
}

const isElement = (v: unknown): v is ElementNode =>
  typeof v === "object" && v !== null && (v as ElementNode).type === "element";
const VALUE = /^(dp|dv)_[a-z0-9]+(_[a-z0-9]+)*$/;
const SERIES = /^sr_[a-z0-9]+(_[a-z0-9]+)*$/;

/**
 * The key figures and charts a page's own page.oui places, for a listing: every element with a label and a dp_/dv_
 * value is a figure (KeyFigure), and every named element with a title and sr_ series is a chart (the highlighted
 * series, else the first). The home template calls this on a slot page's parsed page.oui.
 */
export function listingParts(root: unknown): Pick<ListedPage, "figures" | "charts"> {
  const figures: Record<string, ListedFigure> = {};
  const charts: Record<string, ListedChart> = {};
  const visit = (v: unknown): void => {
    if (Array.isArray(v)) {
      for (const x of v) visit(x);
      return;
    }
    if (!isElement(v)) return;
    const p = v.props as Record<string, unknown>;
    if (typeof p.label === "string" && typeof p.value === "string" && VALUE.test(p.value) && !figures[p.value]) {
      figures[p.value] = { label: p.label, trend: typeof p.trend === "string" ? p.trend : null };
    }
    const series = (Array.isArray(p.series) ? p.series : [p.series]).filter(
      (s): s is string => typeof s === "string" && SERIES.test(s),
    );
    if (v.statementId && typeof p.title === "string" && series.length > 0) {
      const highlight = typeof p.highlight === "string" && series.includes(p.highlight) ? p.highlight : null;
      charts[v.statementId] = { title: p.title, series: highlight ?? (series[0] as string) };
    }
    for (const x of Object.values(p)) visit(x);
  };
  visit(root);
  return { figures, charts };
}
