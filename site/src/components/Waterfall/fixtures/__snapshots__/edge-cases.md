---
title: "Waterfall: edge-cases"
fly_title: "Component fixture"
summary: "The Waterfall component's \"edge-cases\" fixture, rendered by the page renderer."
generator: "news-renderer 1.0.0"
---

Component fixture

# Waterfall: edge-cases

The Waterfall component's "edge-cases" fixture, rendered by the page renderer.

### A net loss after an operating loss and a tax credit

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

### A line with no value breaks the running total until the next total

Synthetic income statement, US$ m

| Item | 2024 (US$ millions) |
| --- | ---: |
| Revenue | 1000 |
| Cost of revenue | 700 |
| Restructuring charges | No data |
| Operating expenses | 450 |
| Operating loss | -150 |
| Unknown reference | No data |
| Net loss | -130 |

### Lines left out do not add up to the total

Synthetic income statement, US$ m

| Item | 2024 (US$ millions) |
| --- | ---: |
| Revenue | 1000 |
| Cost of revenue | 700 |
| Operating loss | -150 |

The lines shown do not add up to Operating loss: the source has lines not shown here.

### Every line unknown

No data.

### A line in another unit is left out, never converted

Synthetic income statement, US$ m

| Item | 2024 (US$ millions) |
| --- | ---: |
| Revenue | 1000 |
| Cost of revenue | 700 |
| Gross profit | 300 |

## Sources

- [Synthetic fixture series for component tests](https://example.org/statistics/synthetic-fixtures). News test fixtures. CC BY 4.0. Retrieved 16 November 2026.
