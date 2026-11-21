---
title: "Slope: edge-cases"
fly_title: "Component fixture"
summary: "The Slope component's \"edge-cases\" fixture, rendered by the page renderer."
generator: "news-renderer 1.0.0"
---

Component fixture

# Slope: edge-cases

The Slope component's "edge-cases" fixture, rendered by the page renderer.

### Greater China was the only region where sales fell

Net sales by region, fiscal years, US$ bn

| Item | fiscal 2023 (US$ millions) | fiscal 2024 (US$ millions) |
| --- | ---: | ---: |
| Americas | 162560 | 167045 |
| Europe | 94294 | 101328 |
| Greater China | 72559 | 66952 |
| Japan | 24257 | 25052 |
| Rest of Asia Pacific | 29615 | 30658 |

### Close values are nudged apart and never overlap

Synthetic values, %

| Item | 2023 (%) | 2024 (%) |
| --- | ---: | ---: |
| Other | 4.1 | 2.9 |
| Third | 5.4 | 3.0 |
| Fourth | 4.4 | 3.8 |
| Zero series, with a long name that wraps | 0 | 0.4 |
| Negative | -2.6 | -1.8 |
| Gaps | -0.4 | 0.9 |
| Restricted | 11.8 | 12.1 |
| Other again | 8.0 | 4.1 |
| Third again | 6.1 | 5.4 |

### A missing end is No data, never a zero

Synthetic values, %

| Item | 2021 (%) | 2022 (%) |
| --- | ---: | ---: |
| Second not published | 2.2 | No data |
| Both present | 4.7 | 8.0 |
| First unknown | No data | 6.1 |
| Different unit | No data | No data |

### Every value unknown

No data.

## Sources

- [Apple Inc. Form 10-K, fiscal 2024](https://www.sec.gov/Archives/edgar/data/320193/000032019324000123/aapl-20240928.htm). Apple Inc. Public filing. Retrieved 1 November 2024.
- [Synthetic fixture series for component tests](https://example.org/statistics/synthetic-fixtures). News test fixtures. CC BY 4.0. Retrieved 16 November 2026.
- [Licensed fixture series](https://example.org/statistics/synthetic-fixtures). News test fixtures. No redistribution. Retrieved 16 November 2026.
