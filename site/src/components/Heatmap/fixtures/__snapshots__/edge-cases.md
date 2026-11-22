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

### Sixty months in one row: no value has room, so none is written

Synthetic index, monthly, 2020=100

| Series | Jan 2020 (Index 2020=100) | Feb 2020 (Index 2020=100) | Mar 2020 (Index 2020=100) | Apr 2020 (Index 2020=100) | May 2020 (Index 2020=100) | Jun 2020 (Index 2020=100) | Jul 2020 (Index 2020=100) | Aug 2020 (Index 2020=100) | Sep 2020 (Index 2020=100) | Oct 2020 (Index 2020=100) | Nov 2020 (Index 2020=100) | Dec 2020 (Index 2020=100) | Jan 2021 (Index 2020=100) | Feb 2021 (Index 2020=100) | Mar 2021 (Index 2020=100) | Apr 2021 (Index 2020=100) | May 2021 (Index 2020=100) | Jun 2021 (Index 2020=100) | Jul 2021 (Index 2020=100) | Aug 2021 (Index 2020=100) | Sep 2021 (Index 2020=100) | Oct 2021 (Index 2020=100) | Nov 2021 (Index 2020=100) | Dec 2021 (Index 2020=100) | Jan 2022 (Index 2020=100) | Feb 2022 (Index 2020=100) | Mar 2022 (Index 2020=100) | Apr 2022 (Index 2020=100) | May 2022 (Index 2020=100) | Jun 2022 (Index 2020=100) | Jul 2022 (Index 2020=100) | Aug 2022 (Index 2020=100) | Sep 2022 (Index 2020=100) | Oct 2022 (Index 2020=100) | Nov 2022 (Index 2020=100) | Dec 2022 (Index 2020=100) | Jan 2023 (Index 2020=100) | Feb 2023 (Index 2020=100) | Mar 2023 (Index 2020=100) | Apr 2023 (Index 2020=100) | May 2023 (Index 2020=100) | Jun 2023 (Index 2020=100) | Jul 2023 (Index 2020=100) | Aug 2023 (Index 2020=100) | Sep 2023 (Index 2020=100) | Oct 2023 (Index 2020=100) | Nov 2023 (Index 2020=100) | Dec 2023 (Index 2020=100) | Jan 2024 (Index 2020=100) | Feb 2024 (Index 2020=100) | Mar 2024 (Index 2020=100) | Apr 2024 (Index 2020=100) | May 2024 (Index 2020=100) | Jun 2024 (Index 2020=100) | Jul 2024 (Index 2020=100) | Aug 2024 (Index 2020=100) | Sep 2024 (Index 2020=100) | Oct 2024 (Index 2020=100) | Nov 2024 (Index 2020=100) | Dec 2024 (Index 2020=100) |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Synthetic index | 100.0 | 101.9 | 103.8 | 105.6 | 107.1 | 108.5 | 109.6 | 110.3 | 110.8 | 110.9 | 110.8 | 110.3 | 109.6 | 108.7 | 107.6 | 106.4 | 105.1 | 103.9 | 102.8 | 101.8 | 100.9 | 100.4 | 100.1 | 100.1 | 100.4 | 101.1 | 102.0 | 103.3 | 104.7 | 106.4 | 108.3 | 110.2 | 112.1 | 114.0 | 115.9 | 117.5 | 118.9 | 120.1 | 121.0 | 121.6 | 121.9 | 121.9 | 121.5 | 120.9 | 120.1 | 119.0 | 117.9 | 116.6 | 115.4 | 114.2 | 113.1 | 112.3 | 111.6 | 111.2 | 111.1 | 111.3 | 111.8 | 112.6 | 113.7 | 115.1 |

## Sources

- [Synthetic fixture series for component tests](https://example.org/statistics/synthetic-fixtures). News test fixtures. CC BY 4.0. Retrieved 16 November 2026.
- [Licensed fixture series](https://example.org/statistics/synthetic-fixtures). News test fixtures. No redistribution. Retrieved 16 November 2026.
