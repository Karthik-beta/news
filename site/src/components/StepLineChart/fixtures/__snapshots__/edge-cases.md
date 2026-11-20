---
title: "StepLineChart: edge-cases"
fly_title: "Component fixture"
summary: "The StepLineChart component's \"edge-cases\" fixture, rendered by the page renderer."
generator: "news-renderer 1.0.0"
---

Component fixture

# StepLineChart: edge-cases

The StepLineChart component's "edge-cases" fixture, rendered by the page renderer.

### A path with a missing decision

Policy rate, %

| Period | Policy rate (%) |
| --- | ---: |
| 26 January 2022 | 0.25 |
| 16 March 2022 | No data |
| 4 May 2022 | No data |
| 15 June 2022 | 1.75 |

### A single decision

Turnout, %

| Period | Turnout (%) |
| --- | ---: |
| 2024 | 42.5 |

### Periods without dates are spaced evenly

Growth, %

| Period | Comparison measure (%) |
| --- | ---: |
| 2019 | 2.0 |
| 2020 | 1.1 |
| 2021 | 4.7 |
| 2022 | 8.0 |
| 2023 | 4.1 |
| 2024 | 2.9 |

### An unknown series

No data.

### A marker the series does not have

Federal funds target range, upper limit, %

| Period | Federal funds rate, upper limit (%) |
| --- | ---: |
| 26 January 2022 | 0.25 |
| 16 March 2022 | 0.50 |
| 4 May 2022 | 1.00 |
| 15 June 2022 | 1.75 |
| 27 July 2022 | 2.50 |
| 21 September 2022 | 3.25 |
| 2 November 2022 | 4.00 |
| 14 December 2022 | 4.50 |
| 1 February 2023 | 4.75 |
| 22 March 2023 | 5.00 |
| 3 May 2023 | 5.25 |
| 26 July 2023 | 5.50 |
| 18 September 2024 | 5.00 |
| 7 November 2024 | 4.75 |
| 18 December 2024 | 4.50 |

### Inflation ended 2024 lower than it began

Consumer prices, change on a year earlier, 2024, %

| Period | Consumer prices (%) |
| --- | ---: |
| Jan 2024 | 3.1 |
| Feb 2024 | 3.2 |
| Mar 2024 | 3.5 |
| Apr 2024 | 3.4 |
| May 2024 | 3.3 |
| Jun 2024 | 3.0 |
| Jul 2024 | 2.9 |
| Aug 2024 | 2.5 |
| Sep 2024 | 2.4 |
| Oct 2024 | 2.6 |
| Nov 2024 | 2.7 |
| Dec 2024 | 2.9 |

## Sources

- [Federal Reserve Board, FOMC statements, 2022 to 2024](https://www.federalreserve.gov/monetarypolicy/fomccalendars.htm). Board of Governors of the Federal Reserve System. Public domain. Retrieved 6 January 2025.
- [Synthetic fixture series for component tests](https://example.org/statistics/synthetic-fixtures). News test fixtures. CC BY 4.0. Retrieved 16 November 2026.
- [Consumer Price Index, all urban consumers, 2024](https://www.bls.gov/cpi/). U.S. Bureau of Labor Statistics. Public domain. Retrieved 15 January 2025.
