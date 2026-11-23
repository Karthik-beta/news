---
title: "DotPlot: edge-cases"
fly_title: "Component fixture"
summary: "The DotPlot component's \"edge-cases\" fixture, rendered by the page renderer."
generator: "news-renderer 1.1.0"
---

Component fixture

# DotPlot: edge-cases

The DotPlot component's "edge-cases" fixture, rendered by the page renderer.

### Greater China was the only region where sales fell

Net sales by region, fiscal years, US$ bn

| Category | fiscal 2023 (US$ millions) | fiscal 2024 (US$ millions) |
| --- | ---: | ---: |
| Americas | 162560 | 167045 |
| Greater China | 72559 | 66952 |
| Japan | 24257 | 25052 |

### A missing value is No data, never a zero

Synthetic values, %

| Category | Earlier (%) | Later (%) |
| --- | ---: | ---: |
| Both present | 1.5 | 2.2 |
| Second not published | 2.2 | No data |
| First unknown | No data | 0.9 |
| Equal values | 0.9 | 0.9 |
| A long label that has to wrap on a phone, with a fall | 2.2 | -0.4 |

### Negative, zero and positive values on one scale

Synthetic values, %

| Category | Value (%) |
| --- | ---: |
| A rise | 3.4 |
| No change | 0 |
| A fall | -5.7 |
| Different unit | No data |

### One dot

Turnout, %

| Category | Value (%) |
| --- | ---: |
| Turnout | 42.5 |

### Every value unknown

No data.

## Sources

- [Apple Inc. Form 10-K, fiscal 2024](https://www.sec.gov/Archives/edgar/data/320193/000032019324000123/aapl-20240928.htm). Apple Inc. Public filing. Retrieved 1 November 2024.
- [Synthetic fixture series for component tests](https://example.org/statistics/synthetic-fixtures). News test fixtures. CC BY 4.0. Retrieved 16 November 2026.
