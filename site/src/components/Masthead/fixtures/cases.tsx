import type { FixtureCase } from "../../contract";
import { Masthead, SECTIONS } from "../Masthead";

// Section hubs and search are not built yet; fixture links point at the home page (with a query to tell them
// apart) so the built fixture passes the internal link check. The labels are the real ones. Several mastheads
// share this page, so each names its navigation differently; a real page has one, named "Sections".
const at = (label: string) => `/?section=${encodeURIComponent(label.toLowerCase())}`;
const sections = SECTIONS.map((s) => ({ label: s.label, href: at(s.label) }));
const search = "/?search";

export const cases: FixtureCase[] = [
  { name: "home (no current section)", render: () => <Masthead sections={sections} searchHref={search} /> },
  {
    name: "current section: Finance",
    render: () => (
      <Masthead sections={sections} current={at("Finance")} searchHref={search} navLabel="Sections, Finance case" />
    ),
  },
  {
    name: "one section",
    render: () => (
      <Masthead
        sections={[{ label: "Science", href: at("Science") }]}
        current={at("Science")}
        searchHref={search}
        navLabel="Sections, one-section case"
      />
    ),
  },
  {
    name: "maximum: eight sections with long names (wraps)",
    render: () => (
      <Masthead
        sections={[
          ...sections,
          { label: "Health and medicine", href: at("Health and medicine") },
          { label: "Energy and climate", href: at("Energy and climate") },
          { label: "Government statistics", href: at("Government statistics") },
        ]}
        current={at("Government statistics")}
        searchHref={search}
        navLabel="Sections, eight-section case"
      />
    ),
  },
];
