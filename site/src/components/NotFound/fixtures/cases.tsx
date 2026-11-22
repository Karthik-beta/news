import type { FixtureCase } from "../../contract";
import { SECTIONS } from "../../Masthead/Masthead";
import { NotFound } from "../NotFound";

// Hubs and search are not built yet, so the fixture links and searches the home page. The gallery has its own h1.
const sections = SECTIONS.map((s) => ({ label: s.label, href: `/?section=${s.label.toLowerCase()}` }));

export const cases: FixtureCase[] = [
  { name: "404", render: () => <NotFound sections={sections} searchAction="/" level={2} /> },
];
