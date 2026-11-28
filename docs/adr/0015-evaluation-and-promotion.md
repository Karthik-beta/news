# ADR 0015: Evaluation and promotion (Critic, Audit, Huddle)

- Status: proposed (judges, fresh candidate calls and Huddle's Chief steps are not built yet)
- Date: 2026-11-28
- Issue: #19 (Phase 7): runtime skills, Critic, Audit and Huddle
- Context documents: Bureau Design §4.7 (ranking calibration), §7 (A12), §8.11 (Audit), §8.12 (Huddle), §9.1 (golden sets and judges); ADR 0008 §6.3 (G tier); ADR 0012 (scoring); `pipeline/eval/README.md`; `pipeline/eval/judges.md`

## Context

Every AI step runs on a prompt (a runtime skill in `skills/`) and a model. A change to either can make pages worse in ways no single run shows. Bureau §9.1 asks for golden sets, judges checked against Karthik's labels, and promotion only by approval. Huddle's ranking weights are the same kind of change: a fit to Karthik's pairwise labels that must never move the live scorer without his approval.

## Decision

1. **Golden sets are files in the repository.** `pipeline/eval/<set>/` holds a `set.json` manifest (suite, step, target count, starting targets, `version`) and one case per file, each with provenance pointing at the recorded fixture it came from. JSON Schemas in `pipeline/eval/schemas/` are checked on every load, in CI. Changing a set's cases or targets bumps its `version`, so runs of different versions are never compared. No value is typed: pages in G6 are written in ref tokens and filled from recorded briefs by code, which refuses any digit outside a token.
2. **Deterministic metrics decide; judges only add.** Metrics per suite are computed by code (`news-critic/src/metrics.rs`) from stored outputs (`<set>/outputs/<prompt_version>.jsonl`). A12 judges are binary, one per failure mode, and an Audit uses a judge only after its true-positive and true-negative rates against Karthik's labels are recorded in `pipeline/eval/judges.md`. Until then the `judges` step is skipped, which is the fallback Bureau §7 gives A12.
3. **Audit scores stored outputs first.** The Audit preset loads the sets, scores the candidate's stored outputs, compares each set with the live version of its step, and flags any metric that moved the wrong way for its target. Fresh candidate calls (Ghost jobs for the candidate version, step 3) are a later addition; until they exist, a version with missing outputs fails at `candidate`, and it says which sets are missing.
4. **Promotion is G tier.** `news eval promote` from a terminal is always `gated`. The only path is the Audit run: the `promote` approval in Board, then the `promote` step writes `prompt_promotions` inside Chief's authorised transaction. The latest row per step (A1, A3, A7, A8) is the live version.
5. **Ranking calibration is a constrained fit, adopted by approval.** `news scores propose` fits the six weights of S to Karthik's labels with a Bradley–Terry model by projected gradient ascent: every weight at least zero, the six summing to one, each within ±0.05 of its live value per week. A proposal is stored and changes nothing; `news scores adopt` is G tier, and in Huddle the `apply` step adopts a change only after that change's approval.

## Consequences

- A prompt or model change is reviewable as a diff of a skill plus an Audit report; a regression is traceable to the `prompt_version` stored with each AI output.
- Because the gate is code, an Audit can run in CI on recorded outputs with no AI call and no cost.
- Judges add cost and their own bias; keeping them optional until measured keeps reports honest while labels are missing.
- The ±0.05 weekly bound means a large disagreement with Karthik's labels takes several Huddles to close; that is intended.

## Not yet built

- Fresh candidate calls in Audit, and therefore Audit end to end with a fake `claude`.
- The A12 judges and their rates (`judges.md` waits for Karthik's labels).
- Huddle's preset steps in Chief (`huddle.debrief`, `huddle.calibration`, `huddle.propose`, `huddle.apply`): the pieces exist (`news review weekly`, `news scores label|propose|adopt`, `huddle.questions` and `huddle.answer` over the Hotline), but Chief has no way yet to complete a form step, so the preset cannot run end to end.
