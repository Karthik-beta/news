import type { FixtureCase } from "../../contract";
import { Headline } from "../Headline";

const typical = "Apple's sales grew again in {pd_fy2024}, to {dp_aapl_net_sales_fy2024}";
const long =
  "Services reached a quarter of Apple's sales for the first time while hardware revenue held steady across every region except Greater China";

// The gallery's own h1 is the primitive's name, so article-size cases render at level 2 here.
export const cases: FixtureCase[] = [
  { name: "article (xl), with references", render: () => <Headline text={typical} level={2} /> },
  { name: "short", render: () => <Headline text="Inflation held" level={2} /> },
  { name: "long (about 140 characters)", render: () => <Headline text={long} level={2} /> },
  {
    name: "very long word",
    render: () => (
      <Headline text="Researchers sequenced Pneumonoultramicroscopicsilicovolcanoconiosis samples" level={2} />
    ),
  },
  {
    name: "missing reference reads 'no data'",
    render: () => <Headline text="Sales reached {dp_edge_null}" level={2} />,
  },
  {
    name: "display (home lead), linked",
    render: () => <Headline text={typical} size="display" level={2} href="/?lead" />,
  },
  { name: "m, linked", render: () => <Headline text={typical} size="m" level={3} href="/?story=m" /> },
  { name: "s, linked", render: () => <Headline text={typical} size="s" level={3} href="/?story=s" /> },
];
