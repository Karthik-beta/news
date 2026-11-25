import type { FixtureCase } from "../../contract";
import { FOOTER_LINKS, Footer } from "../Footer";

// The site's own pages (methodology, glossary …) are not built yet; fixture links point at the home page so the
// built fixture passes the internal link check. The labels are the real ones. Two footers share this page, so the
// second names its navigation differently.
const here = FOOTER_LINKS.map((l, i) => ({ label: l.label, href: `/?footer=${i}` }));

export const cases: FixtureCase[] = [
  { name: "footer", render: () => <Footer links={here} /> },
  { name: "one link", render: () => <Footer links={here.slice(0, 1)} navLabel="About this site, one-link case" /> },
];
