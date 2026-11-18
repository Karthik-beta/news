import type { SourceT } from "../../../renderer/bundle/schema";
import type { FixtureCase } from "../../contract";
import { FigureFrame } from "../FigureFrame";

const tenK: SourceT = {
  title: "Apple Inc. Form 10-K, fiscal 2024",
  short: "Form 10-K, fiscal 2024",
  publisher: "Apple Inc.",
  connector: "sec-edgar",
  type: "filing",
  primary: true,
  url: "https://www.sec.gov/Archives/edgar/data/320193/000032019324000123/aapl-20240928.htm",
  licence: { name: "Public filing", redistribution: true },
  retrieved_at: "2024-11-01",
};
const sources = [{ id: "src_aapl_10k_2024", source: tenK }];

const QUARTERS = [
  "Q1 FY23",
  "Q2 FY23",
  "Q3 FY23",
  "Q4 FY23",
  "Q1 FY24",
  "Q2 FY24",
  "Q3 FY24",
  "Q4 FY24",
  "Q1 FY25",
  "Q2 FY25",
  "Q3 FY25",
  "Q4 FY25",
];
const Plot = () => <div className="fixture-plot-placeholder">Plot area</div>;
const table = (
  <table>
    <thead>
      <tr>
        <th scope="col">Fiscal year</th>
        <th scope="col" className="is-number">
          Net sales, $bn
        </th>
      </tr>
    </thead>
    <tbody>
      <tr>
        <th scope="row">fiscal 2024</th>
        <td className="is-number">391.0</td>
      </tr>
    </tbody>
  </table>
);

export const cases: FixtureCase[] = [
  {
    name: "full anatomy",
    render: () => (
      <FigureFrame
        id="frame-full"
        title="Services earns about twice the gross margin of products"
        subtitle="Gross margin as a share of net sales, fiscal 2020–2024, %"
        legend={<p className="text-label">Services, Products</p>}
        sources={sources}
        table={table}
        downloads={{ csv: "#frame-full-title", json: "#frame-full-title" }}
      >
        <Plot />
      </FigureFrame>
    ),
  },
  {
    name: "title only, no source, no actions",
    render: () => (
      <FigureFrame id="frame-min" title="A frame with only a title">
        <Plot />
      </FigureFrame>
    ),
  },
  {
    name: "two sources and a calculation note",
    render: () => (
      <FigureFrame
        id="frame-two"
        title="Sales grew for the first time in two years"
        subtitle="Change in net sales on the previous fiscal year, %"
        sourceNote="Calculated by News from the filings."
        sources={[...sources, { id: "src_aapl_10k_2022", source: { ...tenK, short: "Form 10-K, fiscal 2022" } }]}
      >
        <Plot />
      </FigureFrame>
    ),
  },
  {
    name: "very long title and subtitle",
    render: () => (
      <FigureFrame
        id="frame-long"
        title="The International Monetary Fund lowered its forecast for global growth in 2025 for the second time this year, citing trade"
        subtitle="Real GDP growth, annual percentage change, World Economic Outlook editions from April 2023 to October 2025, as published"
        sources={sources}
      >
        <Plot />
      </FigureFrame>
    ),
  },
  // Added by the tester (news-component-testing): content edge cases for the screen matrix.
  {
    name: "wide data table scrolls inside the frame",
    render: () => (
      <FigureFrame
        id="frame-wide"
        title="Sales by quarter and region"
        subtitle="Net sales, US$ millions, as reported"
        sources={sources}
        table={
          <table>
            <thead>
              <tr>
                <th scope="col">Region</th>
                {QUARTERS.map((q) => (
                  <th key={q} scope="col" className="is-number">
                    {q}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              <tr>
                <th scope="row">Greater China</th>
                {QUARTERS.map((q) => (
                  <td key={q} className="is-number">
                    20,819.5
                  </td>
                ))}
              </tr>
            </tbody>
          </table>
        }
        downloads={{ csv: "#frame-wide-title" }}
      >
        <Plot />
      </FigureFrame>
    ),
  },
  {
    name: "level-3 title, JSON only, unbroken source name",
    render: () => (
      <FigureFrame
        id="frame-h3"
        headingLevel={3}
        title="A frame inside a section"
        sources={[
          {
            id: "src_aapl_10k_2024",
            source: { ...tenK, publisher: "Bundesministeriumfürwirtschaftundklimaschutzstatistikabteilung" },
          },
        ]}
        downloads={{ json: "#frame-h3-title" }}
      >
        <Plot />
      </FigureFrame>
    ),
  },
];
