# ADR 0012: Scoring and selection (Nose and Desk)

- Status: proposed
- Date: 2026-11-23
- Issue: #15 (Phase 4, wave 4), the Nose and Desk part; Taster, the embedding spike and ADR 0011 follow in their own PR
- Context documents: Bureau Design §3.7, §4 (§4.2 signals, §4.3 score, §4.4 selection, §4.6 worked example), §5.8 (idempotency keys); `cli/crates/news-nose/README.md`

## Context

Bureau §4 fixes the formulas: ten signals in [0, 1], the score `S = 100 · Q · (0.30·I + 0.20·M + 0.15·F + 0.15·D + 0.10·N + 0.10·G)`, greedy selection with diminishing returns per bucket, three hard limits and two tiers. What it leaves open is where the inputs come from before Taster exists, how a score stays idempotent, and a handful of edge cases the formulas do not settle. This record settles them so the §4.6 worked example is a test, not a table.

## Decisions

### Inputs: `stories.score_input`

- Nose scores a **story input** (`news_nose::score::StoryInput`, JSON in `stories.score_input`, migration 0400): event type, page type with the source data its magnitude formula needs (a macro series, a rate change, revenue growth history, forecast revisions, science flags, the count of primary sources), categories (first is primary), regions (first is primary), the main entity and other linked entities with their prominence, the source tier, publication time, data point count, whether a 12-point series exists, novelty, the source's `redistribution` flag, parse warnings and `action` (`new` or `update`).
- Taster fills it when it groups candidates (the later #15 PR). Until then `news plan` scores only open stories that have it and answers `precondition` when none do. A malformed input is a warning on that story, not a failed plan.
- Every number in it is source data or our own record, filled by code; Nose never sees a model's opinion.

### Score

- **Formulas exactly as §4.2–§4.3.** Where §4.2 is silent:
  - σ is the population standard deviation.
  - Macro: `σ(Δx)` is over every change between the last 60 observations (20 when there are 4 or fewer a year), the latest change included, as the formula reads. The 10-year rule needs ten years of earlier observations; a new high or low is strictly above or below all of them.
  - Earnings z subtracts the history's mean: `|x − mean| / σ`. Report z is the headline revision over the standard deviation of past revisions, `|r| / σ`, as §4.2 reads (no mean is subtracted, matching the macro row's `|Δ| / σ(Δ)`). Fewer than 2 history points, or σ = 0, is "not computable" and gives **0.5**, the value §4.2 sets for a report; macro with fewer than 3 observations does the same.
  - Policy: any non-zero move under 50 bp scores 0.8; the +0.2 bonus (first move after 6+ holds, or a reversal) applies to moves only.
  - G uses the story's **primary** category and **primary** region with its page type. Actual shares are the pages published in the 7 days before the run, bucketed by their story's input; a bucket with no target has no gap.
  - Entities with no prominence count as 3 (Roster's default); with no linked entity P is 0.
- S is stored as `numeric(5,2)`; tiers and Desk use the stored value.
- **Idempotency** (§5.8: story + scorer version + hour): a plan scores at the run's start hour (`runs.started_at`, else `created_at`, truncated to the hour) and inserts score rows `on conflict do nothing` (the key is story, hour and scorer version, so a new version in the same hour adds its own row). The same run planned twice gives the same rows.
- **The Pitch** is the score row's `explain`: a header line with S, Q, the weights, the version and the tier, then one line per component (I, M, F, D, N, G, Q) saying what produced it.

### Config and seeds

- Scorer config v1 is `data/seeds/scorer-v1.toml` (the six weights and the freshness half-lives); event weights and coverage targets are `data/seeds/event_weights.toml` and `coverage_targets.toml`. All three are compiled into the binary and checked on load (weights sum to 1, each target table sums to 1). A score's `scorer_version` therefore names exactly what produced it; a change is a new version, which calibration (#19) proposes and Karthik adopts.
- The `event_weights` table (migration 0004) holds the same rows; a test keeps the seed file and the migration in step. Nose reads the seed, not the table, until #19 makes versions editable data.

### Desk

- **Greedy by `adjusted`**, exactly §4.4: `n_cat` and `n_region` are the story's most-covered bucket among those it carries; picking it counts every one. Ties go to the higher S, then the lower story id. A linear scan per pick replaces the lazy max-heap; it is plenty for a few thousand open stories.
- **Hard limits** in integer arithmetic: category cap `max(2, ⌈35 · target / 100⌉)` on the primary category, 2 stories per main entity, Tier 1 quota `max(1, ⌊20 · target / 100⌋)`.
- **Stopping**: the target of new pages is met, or no unpicked story with S ≥ 40 is allowed by the limits.
- **Tiers**: tier 1 if S ≥ 70, or main-entity prominence 1 and S ≥ 60; tier 2 if S ≥ 40; below 40 the story stays queued. A story that earned tier 1 after the quota is planned as tier 2.
- **Updates** (`action = update`) are planned after the new pages, outside the breadth caps, highest S first, at most `⌊10 · target / 100⌋` of them. They share the Tier 1 quota: the quota is a cost limit on AI work, not a breadth cap, so an update after the quota is planned as tier 2. Value-only `revise` items (code re-renders, no AI) are Rewrite's (#20) and are not planned here yet.
- **Queued** means no `run_plans` row: the story stays `open` and is re-scored next run (its freshness decays and its bucket's coverage changes). `news plan` lists each held story with S ≥ 40 and the reason (category cap, entity limit, target reached, update cap) and counts those below the floor. The TD's `queue` and `clusters` tables are not used by Nose or Desk.
- **Re-planning a run** replaces its `run_plans` rows in one transaction with the scores, and only while the Budget is still Desk's to change: the run is `queued` or `running` at its `plan` step, has no approval, and no step after `plan` has started (otherwise `precondition`, since an approval's hash covers the plan). A re-plan that produces byte-identical rows is a no-op. The run's target is capped at 1000 pages (`usage` error above that).

### Logging

Every score of a run, selected or not, goes to ClickHouse `news.scores` with the run id, components, S, `selected` and the planned tier (Bureau §4.7), through Ticker's writer (`ClickHouse::insert_rows`). It is written only when the plan inserted new score rows, so a re-run adds nothing (even a first plan with no picks); as with all telemetry, a ClickHouse outage never fails the command.

### The worked example

`cli/crates/news-nose/tests/fixtures/worked-example.json` holds the §4.6 stories as inputs (plus US PPI, tuned to S = 70.0). The tests reproduce the five scores (S to one decimal, components within 0.001 of the printed values), the four picks and their planned tiers, and a target-10 case that exercises the entity limit and a Tier 1 quota of 2. §4.6's adjusted values were worked from S rounded to one decimal (61.3, 46.3); from S to two decimals they are 61.24 and 46.25, which changes no pick.

Target 10 (category cap 4, entity limit 2, Tier 1 quota 2, update cap 1), with the six stories plus an ECB decision (73), an Apple 8-K (64) and an Apple EU ruling (62), an Australian CPI (55), an item below the floor (35) and two updates (62, 58): CPI and ECB take the Tier 1 places; IMF WEO, Apple results and RBI earned tier 1 and are planned tier 2; Economy reaches its cap of 4, blocking US PPI and Australian CPI; then the preprint and the Apple EU ruling, after which the Apple 8-K is Apple's third story and is blocked by the entity limit. One update fits the cap and is planned tier 2 (quota used); the other stays queued.

## Consequences

- `news plan --run`, `news scores explain` and `news scores weights` are built; `scores propose|label` move to #19, where calibration lives.
- The Dispatch job kinds `story.score` and `run.select` still fail as not built: Chief's Edition step calls Desk when step handlers land.
- Taster must write `stories.score_input` in this shape; its PR owns the 0401+ migrations.
- Backfill's "F ignored" (Catchup) is not built: it needs a flag on the run, added with that preset.
