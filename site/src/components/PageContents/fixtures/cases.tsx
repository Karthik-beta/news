import type { FixtureCase } from "../../contract";
import { PageContents } from "../PageContents";

// Each case brings the headings its links point at, so every link has a target on the fixture page.
const Targets = ({ ids }: { ids: string[] }) => (
  <>
    {ids.map((id) => (
      <p key={id} id={id} className="text-label">
        Target {id}
      </p>
    ))}
  </>
);

// A page has one "On this page" block, so the gallery renders one: a second would repeat the landmark name
// (html-validate unique-landmark).
export const cases: FixtureCase[] = [
  {
    name: "sections and charts, a title with a reference, a long item that wraps",
    render: () => (
      <>
        <PageContents
          items={[
            { id: "contents-sales", text: "Why sales rose in {pd_fy2024}" },
            { id: "contents-chart", text: "Sales grew again in {pd_fy2024} after a dip" },
            { id: "contents-method", text: "How this page was made" },
            {
              id: "contents-long",
              text: "A section heading long enough to wrap over several lines in the narrow rail, with a long word: Pneumonoultramicroscopicsilicovolcanoconiosis",
            },
          ]}
        />
        <Targets ids={["contents-sales", "contents-chart", "contents-method", "contents-long"]} />
      </>
    ),
  },
  { name: "no items renders nothing", render: () => <PageContents items={[]} /> },
];
