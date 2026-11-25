# pipeline/eval

The golden sets Critic runs (Bureau Design §9.1, §8.11). `news eval run <suite>` checks them; `news eval run <suite> --prompt-version <v>` scores that version's stored outputs; `news eval report e<N>` prints a run back.

| Set | Suite | Step | Cases | Target | Contents |
| --- | --- | --- | --- | --- | --- |
| `g1` | writer | A1 | 16 | 35 | The six recorded Fixer golden briefs (`cli/crates/news-fixer/tests/golden/`), plus injection and missing-data variants of them |
| `g2` | classify | A3 | 106 | 200 | Every distinct recorded item in the connector and Taster fixtures, with labels drafted by Opus (`labelled_by: opus-draft`) for Karthik to confirm |
| `g5` | curator | A8 | 4 | 4 | Busy week, quiet week, one region dominating, withdrawn lead, assembled from recorded titles |
| `g6` | spotcheck | A7 | 52 | 50 | Pages written in ref tokens over G1 briefs, each with one seeded fault, and 11 clean controls |

## Layout

- `schemas/`: JSON Schemas for `set.json`, each set's cases and the stored outputs. CI checks every case on load (`news-critic` tests); a G1 brief must also validate against `contracts/cli/ai/brief.v1.schema.json`.
- `<set>/set.json`: the manifest, with the suite, the step, the target case count and the starting metric targets.
- `<set>/cases/<id>.json`: one case per file, named by its id. Every case has `provenance`: the recorded file it comes from and the edit made, if any.
- `<set>/outputs/<prompt_version>.jsonl`: recorded AI outputs for a version, one case per line (`schemas/output.schema.json`). Only Audit writes them; they are never hand-written.

## Rules

- **No typed values.** Every value a page would show comes from a recorded file. G6 pages are written with ref tokens (`{dp_…}`, `{dv_…}`, `{pd_…}`); code fills them from the G1 brief as recorded and refuses any digit outside a token.
- **Versioning.** Changing a set's cases or targets bumps `version` in its `set.json`, so reports from different versions are never compared as if they were the same set.
- **Labels.** Labels Opus drafted say so (`opus-draft`). Karthik confirms them by changing the label to `karthik`; the report counts every case either way.

## Metrics

All are computed by code (`cli/crates/news-critic/src/metrics.rs`). A case with no output, or an output of the wrong shape, counts as failed.

| Suite | Metrics |
| --- | --- |
| writer | `valid_first_try`, `valid_after_repair`, `c4_c5_after_repair` (count), `tokens_per_page`, from the job's `ai_calls` rows |
| classify | `page_type_accuracy`, `region_accuracy`, `category_f1` (micro) |
| curator | `check_pass_rate` (the `news home check` rules), `excluded_violations`, `mean_categories`, `mean_regions` |
| spotcheck | `a7_recall` (an issue whose quoted text overlaps the seeded fault), `a7_kind_match`, `false_flag_rate` (issues pointing at no seeded fault), `control_flag_rate` |

## Gaps

- G1 has 16 of 35 briefs: the recorded fixtures give one brief per page type and none for explainer. No restatement or negative-value case yet, because no recorded fact has one.
- G2 has 106 of 200 items, which is every distinct recorded item. More need recorded sources (and earnings items, which it has none of).
- G3 (story pairs) and G7 (entities) are Phase 4 labelling. The A12 judges and their checks against Karthik's labels (`judges.md`) come with Audit's AI steps.
