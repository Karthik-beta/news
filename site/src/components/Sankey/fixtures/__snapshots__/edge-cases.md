---
title: "Sankey: edge-cases"
fly_title: "Component fixture"
summary: "The Sankey component's \"edge-cases\" fixture, rendered by the page renderer."
generator: "news-renderer 1.1.0"
---

Component fixture

# Sankey: edge-cases

The Sankey component's "edge-cases" fixture, rendered by the page renderer.

### Small segments and costs fold into Other

Synthetic income statement, US$ m

| Item | 2024 (US$ millions) |
| --- | ---: |
| Segment A | 600 |
| Segment B | 250 |
| Segment C | 120 |
| Segment D | 15 |
| Segment E | 8 |
| Segment F | 4 |
| Segment G | 3 |
| Revenue | 1000 |
| Cost of revenue | 550 |
| Gross profit | 450 |
| Operating expenses | 200 |
| Impairment | 6 |
| Litigation | 4 |
| Operating profit | 240 |
| Tax | 50 |
| Net profit | 190 |

### Without segments the flows start at net sales

Net sales to operating income, fiscal 2024, US$ bn

| Item | fiscal 2024 (US$ millions) |
| --- | ---: |
| Total net sales | 391035 |
| Total cost of sales | 210352 |
| Gross margin | 180683 |
| Research and development | 31370 |
| Selling, general and administrative | 26097 |
| Operating income | 123216 |

### A loss cannot flow, so the waterfall shows at every width

Synthetic income statement, US$ m

| Item | 2024 (US$ millions) |
| --- | ---: |
| Revenue | 1000 |
| Cost of revenue | 700 |
| Gross profit | 300 |
| Operating expenses | 450 |
| Operating loss | -150 |
| Income tax | -20 |
| Net loss | -130 |

### Lines that do not add up are shown as a waterfall with a note

Net sales to operating income, fiscal 2024, US$ bn

| Item | fiscal 2024 (US$ millions) |
| --- | ---: |
| Total net sales | 391035 |
| Total cost of sales | 210352 |
| Gross margin | 180683 |
| Research and development | 31370 |
| Operating income | 123216 |

The lines shown do not add up to Operating income: the source has lines not shown here.

### Every line unknown

No data.

## Sources

- [Synthetic fixture series for component tests](https://example.org/statistics/synthetic-fixtures). News test fixtures. CC BY 4.0. Retrieved 16 November 2026.
- [Apple Inc. Form 10-K, fiscal 2024](https://www.sec.gov/Archives/edgar/data/320193/000032019324000123/aapl-20240928.htm). Apple Inc. Public filing. Retrieved 1 November 2024.
