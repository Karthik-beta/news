---
title: "DataTable: edge-cases"
fly_title: "Component fixture"
summary: "The DataTable component's \"edge-cases\" fixture, rendered by the page renderer."
generator: "news-renderer 1.1.0"
---

Component fixture

# DataTable: edge-cases

The DataTable component's "edge-cases" fixture, rendered by the page renderer.

### Gaps and unpublished values say No data

Growth, %

| Period | Growth (%) | Not yet published (%) |
| --- | ---: | ---: |
| 2019 | 1.5 | No data |
| 2020 | No data | No data |
| 2021 | 2.2 | No data |
| 2022 | No data | No data |
| 2023 | -0.4 | No data |
| 2024 | 0.9 | No data |

### Very large and very small values as reported

Synthetic measures

| Period | A very large amount (US$) | A very small concentration (mol/L) | Change in output (%) |
| --- | ---: | ---: | ---: |
| 2019 | 123456789012345678 | 0.000000012 | -5.7 |
| 2020 | 98765432109876543 | 0.000000015 | -3.2 |
| 2021 | 150000000000000000 | 0.000000009 | -1.1 |
| 2022 | 87000000000000000 | 0.000000011 | -0.4 |
| 2023 | 132500000000000000 | 0.000000018 | -2.6 |
| 2024 | 141000000000000000 | 0.000000014 | -1.8 |

### Rows of series in another unit are left out

Synthetic measures, %

| Series | 2019 (%) | 2020 (%) | 2021 (%) | 2022 (%) | 2023 (%) | 2024 (%) |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Comparison measure | 2.0 | 1.1 | 4.7 | 8.0 | 4.1 | 2.9 |

### An unknown series

No data.

### A licence that forbids downloads

Licensed measure, %

| Period | Licensed measure (%) |
| --- | ---: |
| 2019 | 9.1 |
| 2020 | 9.8 |
| 2021 | 10.4 |
| 2022 | 10.2 |
| 2023 | 11.8 |
| 2024 | 12.1 |

### A single period

Turnout, %

| Period | Turnout (%) |
| --- | ---: |
| 2024 | 42.5 |

### Output fell in every year

Change in output, %

| Period | Change in output (%) |
| --- | ---: |
| 2019 | -5.7 |
| 2020 | -3.2 |
| 2021 | -1.1 |
| 2022 | -0.4 |
| 2023 | -2.6 |
| 2024 | -1.8 |

### Growth ended lower than it began

Synthetic measures, %

| Series | 2019 (%) | 2020 (%) | 2021 (%) | 2022 (%) | 2023 (%) | 2024 (%) |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Growth | 1.5 | No data | 2.2 | No data | -0.4 | 0.9 |
| Comparison measure | 2.0 | 1.1 | 4.7 | 8.0 | 4.1 | 2.9 |

## Sources

- [Synthetic fixture series for component tests](https://example.org/statistics/synthetic-fixtures). News test fixtures. CC BY 4.0. Retrieved 16 November 2026.
- [Licensed fixture series](https://example.org/statistics/synthetic-fixtures). News test fixtures. No redistribution. Retrieved 16 November 2026.
