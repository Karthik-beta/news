// The site footer (design/components.md, "Footer"): the wash band with a hairline on top; the wordmark; a row of
// links; then the description, the accuracy note and the licence line (design/content-style.md, "The fixed lines").
// A page-template part.

import type { NavLink } from "../Masthead/Masthead";
import { WORDMARK } from "../Masthead/Masthead";

export const FOOTER_LINKS: readonly NavLink[] = [
  { label: "How pages are made", href: "/methodology/" },
  { label: "Glossary", href: "/glossary/" },
  { label: "Data access and MCP", href: "/data/" },
  { label: "Report an error", href: "/report/" },
  { label: "Privacy", href: "/privacy/" },
];

export const SITE_DESCRIPTION = "Visual, fully sourced pages for major company, economic and science reports.";
export const ACCURACY_NOTE =
  "Built from public sources with AI and checked automatically. It may contain errors; check the linked source before relying on a figure.";
export const LICENCE_LINE =
  "Text and charts are CC BY 4.0. Data keeps its original source licence, shown on each page.";

export interface FooterProps {
  links?: readonly NavLink[];
  homeHref?: string;
  /** The navigation landmark's name. Change it only where two footers share a page (fixture galleries). */
  navLabel?: string;
}

export function Footer({ links = FOOTER_LINKS, homeHref = "/", navLabel = "About this site" }: FooterProps) {
  return (
    <footer className="site-footer">
      <div className="site-footer__inner">
        <a className="site-footer__wordmark" href={homeHref}>
          {WORDMARK}
        </a>
        <nav aria-label={navLabel}>
          <ul className="site-footer__links">
            {links.map((l) => (
              <li key={l.href}>
                <a href={l.href}>{l.label}</a>
              </li>
            ))}
          </ul>
        </nav>
        <div className="site-footer__notes">
          <p>{SITE_DESCRIPTION}</p>
          <p>{ACCURACY_NOTE}</p>
          <p>{LICENCE_LINE}</p>
        </div>
      </div>
    </footer>
  );
}
