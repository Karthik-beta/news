---
title: "Heatmap: edge-cases"
fly_title: "Component fixture"
summary: "The Heatmap component's \"edge-cases\" fixture, rendered by the page renderer."
generator: "news-renderer 1.0.0"
---

Component fixture

# Heatmap: edge-cases

The Heatmap component's "edge-cases" fixture, rendered by the page renderer.

### Seven measures by year, with gaps, zeros and negatives

Synthetic values, %

| Series | 2019 (%) | 2020 (%) | 2021 (%) | 2022 (%) | 2023 (%) | 2024 (%) |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Change in the index | 0 | 1.2 | 0 | -0.8 | 0 | 0.4 |
| Change in output | -5.7 | -3.2 | -1.1 | -0.4 | -2.6 | -1.8 |
| Growth | 1.5 | No data | 2.2 | No data | -0.4 | 0.9 |
| Comparison measure | 2.0 | 1.1 | 4.7 | 8.0 | 4.1 | 2.9 |
| Third measure | 1.4 | 0.2 | 2.6 | 6.1 | 5.4 | 3.0 |
| A fourth measure with a long name | 3.3 | 2.9 | 3.1 | 5.2 | 4.4 | 3.8 |
| Licensed measure | 9.1 | 9.8 | 10.4 | 10.2 | 11.8 | 12.1 |

### One cell

Turnout, %

| Series | 2024 (%) |
| --- | ---: |
| Turnout | 42.5 |

### A series in another unit is left out, never converted

Synthetic values, %

| Series | 2019 (%) | 2020 (%) | 2021 (%) | 2022 (%) | 2023 (%) | 2024 (%) |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Comparison measure | 2.0 | 1.1 | 4.7 | 8.0 | 4.1 | 2.9 |

### Nothing published yet

Synthetic values, %

| Series | 2019 (%) | 2020 (%) | 2021 (%) | 2022 (%) | 2023 (%) | 2024 (%) |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Not yet published | No data | No data | No data | No data | No data | No data |

### Every series unknown

No data.

## Sources

- [Synthetic fixture series for component tests](https://example.org/statistics/synthetic-fixtures). News test fixtures. CC BY 4.0. Retrieved 16 November 2026.
- [Licensed fixture series](https://example.org/statistics/synthetic-fixtures). News test fixtures. No redistribution. Retrieved 16 November 2026.
