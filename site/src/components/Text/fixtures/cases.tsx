import type { FixtureCase } from "../../contract";
import { TEXT_VARIANTS, Text } from "../Text";

// Every variant with realistic copy, plus the edge cases: a very long headline and an unbroken long word.
const SAMPLE: Record<string, string> = {
  wordmark: "News",
  display: "Services reached a quarter of Apple's sales in fiscal 2024",
  "headline-xl": "Services reached a quarter of Apple's sales in fiscal 2024",
  "headline-l": "Sales by region",
  "headline-m": "America's inflation eased for a third month",
  "headline-s": "The Reserve Bank of India held its policy rate",
  standfirst: "Net sales rose after two flat years, led by services. Greater China was the only region to shrink.",
  "lead-summary": "Net sales rose after two flat years, led by services.",
  body: "Apple reports its results by fiscal year, which ends on the last Saturday of September.",
  "body-s": "Net sales rose after two flat years.",
  fly: "Apple's annual results",
  "chart-title": "Greater China was the only region where sales fell",
  "chart-subtitle": "Net sales by reportable segment, US$ millions, as reported",
  figure: "$391.0bn",
  "figure-s": "2.4%",
  ui: "Show the data table",
  label: "Net sales",
  source: "Source: Apple Inc., Form 10-K for fiscal 2024",
};

export const cases: FixtureCase[] = [
  ...TEXT_VARIANTS.map((variant) => ({
    name: variant,
    render: () => <Text variant={variant}>{SAMPLE[variant]}</Text>,
  })),
  {
    name: "long headline",
    render: () => (
      <Text as="h2" variant="headline-xl">
        The International Monetary Fund lowered its forecast for global growth in 2025 for the second time this year
      </Text>
    ),
  },
  {
    name: "long unbroken word",
    render: () => (
      <Text variant="body">
        Pneumonoultramicroscopicsilicovolcanoconiosis and
        https://www.sec.gov/Archives/edgar/data/320193/000032019324000123/aapl-20240928.htm wrap inside the column.
      </Text>
    ),
  },
  {
    name: "tabular figures",
    render: () => (
      <Text variant="label" tabular>
        1,111.1 / 999.9 / −5.7
      </Text>
    ),
  },
];
