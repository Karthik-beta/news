# news-nose: Nose and Desk

Nose scores each open story 0–100 by code and explains the score (the Pitch); Desk fills a run breadth-first into the Budget (`run_plans`). Design: Bureau Design §3.7 and §4; decisions: [ADR 0012](../../../docs/adr/0012-scoring-and-selection.md).

| Module | What |
| --- | --- |
| `config` | Scorer config v1 and the seeds, compiled in |
| `score` | `StoryInput`, the ten signals, S, tiers and the Pitch (pure) |
| `desk` | Greedy selection under the hard limits, and the `news plan` output (pure) |
| `store` | `stories.score_input`, coverage, `scores`, `run_plans` |
| `calibrate` | Huddle's Bradley–Terry fit of the weights, the ±0.05 / sum-to-1 rules, Kendall's τ (pure) |
| `huddle` | `score_labels`, `scorer_versions`, the questions, `propose` and `adopt` |

Commands: `news plan --run <uuid>` (W), `news scores explain <story>` (R), `news scores weights` (R), `news scores label --run <run> <file>` (W), `news scores propose --from-labels` (W), `news scores adopt <version>` (G, always gated from the CLI). Calibration: [ADR 0012](../../../docs/adr/0012-scoring-and-selection.md).

## Signals (all in [0, 1])

| | Formula |
| --- | --- |
| I | `0.5·E + 0.3·P + 0.2·T` |
| E | event weight by `event_type` (`data/seeds/event_weights.toml`) |
| P | max over linked entities of prominence 1 → 1.0, 2 → 0.6, 3 → 0.3 (0 with no entity) |
| T | source tier 1 → 1.0, 2 → 0.9, 3 → 0.6, 4 → 0.3 |
| M | by page type, below |
| F | `2^(−age_h / h)`, age from publication to the run's start hour; h by page type |
| D | `min(1, ln(1+n) / ln 51) · (0.7 + 0.3·s)`, n data points, s = 1 with a 12+ point series |
| N | 1.0 new event key; 0.9 regular series; 0.3 follow-up; `1 − max cos` for unstructured items |
| G | mean of `max(0, (target − actual) / target)` over the primary category, primary region and page type |
| Q | 0.9 without redistribution × 0.9 with parse warnings × 0.8 with the main entity unresolved |

**M** (σ is the population standard deviation; "not computable" gives 0.5):

| Page type | Formula |
| --- | --- |
| macro | `z = |x_t − x_{t−1}| / σ(Δx)` over the last 60 observations (20 if 4 or fewer a year); `1 − e^(−z/2)`; a strict 10-year high or low → at least 0.8 |
| policy | ≥ 50 bp → 1.0; other moves → 0.8; hold with > 20 % statement change → 0.4; hold → 0.3; a move after 6+ holds or a reversal +0.2 (cap 1) |
| earnings | `z = |g − mean| / σ` of year-on-year revenue growth against the last 12 quarters; `1 − e^(−z/2)`; loss↔profit flip or restatement → at least 0.7 |
| report | `z = |r − mean| / σ` of the headline revision against past revisions; `1 − e^(−z/2)` |
| science | 0.3, +0.2 open dataset, +0.2 peer-reviewed version of a tracked preprint |
| news | `min(1, log₂(1 + sources) / log₂ 9)` |

## Score

```
S = 100 · Q · (0.30·I + 0.20·M + 0.15·F + 0.15·D + 0.10·N + 0.10·G)      stored to 2 decimals
tier 1: S ≥ 70, or main-entity prominence 1 and S ≥ 60 · tier 2: S ≥ 40 · else queued
```

## Desk

```
adjusted(s) = S · 0.92^n_cat · 0.92^n_region · 0.95^n_type · 0.5^n_entity
category cap = max(2, ⌈0.35·target⌉) on the primary category · entity limit = 2
Tier 1 quota = max(1, ⌊0.20·target⌋) · AI updates ≤ ⌊0.10·target⌋, after new pages, sharing the quota
```

Pick the highest `adjusted` the limits allow until the target is met or none is left; a tier 1 story after the quota is planned tier 2; a story a cap blocks stays queued (no plan row, still `open`).

## Seeds (`data/seeds/`)

| File | Holds |
| --- | --- |
| `scorer-v1.toml` | `version`, the six weights of S, freshness half-lives (hours): news 12, macro 24, policy 24, earnings 48, science 72, report 96 |
| `event_weights.toml` | E by event type, the same rows migration 0004 put in `event_weights` |
| `coverage_targets.toml` | 7-day target shares by category, region and page type |

A change to any of them is a new scorer version (calibration, #19), never an edit to v1.

## Tests

`cargo test -p news-nose`: the §4.6 worked example (five scores, four picks and tiers), the target-10 case, idempotency, every score's version, components and Pitch, and the golden `news plan` output (`tests/golden/plan-worked-example.json`; `NEWS_UPDATE_GOLDEN=1` rewrites it). The Postgres test (`cli/crates/news/tests/postgres/nose.rs`) runs the command against the same fixture and checks the golden, the rows, ClickHouse and a re-run.
