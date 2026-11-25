import type { FixtureCase } from "../../contract";
import { Pagination } from "../Pagination";

// Hubs are not built yet; pages point at the home page with a query (each case its own), so links resolve.
const base = (name: string) => `/?pagination=${name}`;

export const cases: FixtureCase[] = [
  {
    name: "first page of three: no Newer",
    render: () => <Pagination current={1} total={3} base={base("first")} label="Pages, first-page case" />,
  },
  {
    name: "middle of a long run: gaps on both sides",
    render: () => <Pagination current={9} total={24} base={base("middle")} label="Pages, long-run case" />,
  },
  {
    name: "last page: no Older",
    render: () => <Pagination current={24} total={24} base={base("last")} label="Pages, last-page case" />,
  },
  {
    name: "near the start: page two shown instead of a one-page gap",
    render: () => <Pagination current={4} total={12} base={base("near")} label="Pages, near-start case" />,
  },
  { name: "one page renders nothing", render: () => <Pagination current={1} total={1} base={base("one")} /> },
];
