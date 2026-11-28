# A12 judges

One binary judge per failure mode (Bureau Design §9.1, A12 in §7). A judge is trusted in Audit only after its true-positive and true-negative rates against about 100 of Karthik's labels are recorded here. Until then Audit's `judges` step is skipped and reports rest on the deterministic metrics.

## How the rates are made

- **Items.** `news eval draft` stores the outputs to label: every G6 page, filled from its brief, and every G1 output a prompt version stored. An item carries no ground truth (no seeded-fault kind, no validation result) and its id is opaque, so a label is blind.
- **Labels.** In Board's Labelling screen Karthik takes one failure mode at a time and marks each item Pass (the text has none of that failure) or Fail. The label goes over the Hotline (`eval.label`, H tier, Board only) into Postgres. `tone` applies to A1 copy only.
- **Rates.** `news eval export` compares the judge's `pass` with Karthik's label on the same items and rewrites the table below from the database. A pass is the positive class: the true-positive rate is the share of Karthik's passes the judge also passes, the true-negative rate the share of his fails it also fails. The number in brackets is how many of his passes or fails it is a share of. Rates are never typed by hand.
- **Status.** Awaiting labels until a judge has 100; then Awaiting judge until A12 has judged the same items; then Measured. When to trust a measured judge is Karthik's call.

<!-- rates:start (written by `news eval export`; never edit by hand) -->

| Judge (failure mode) | Judges | Karthik's labels | Model | True-positive rate | True-negative rate | Status |
| --- | --- | --- | --- | --- | --- | --- |
| `wrong_direction` | A1 copy (G1), A7 (G6) | 0 | — | — | — | Awaiting labels (0 of 100) |
| `unsupported_claim` | A1 copy (G1), A7 (G6) | 0 | — | — | — | Awaiting labels (0 of 100) |
| `attribution` (misattributed quote) | A1 copy (G1), A7 (G6) | 0 | — | — | — | Awaiting labels (0 of 100) |
| `missing_context` | A1 copy (G1), A7 (G6) | 0 | — | — | — | Awaiting labels (0 of 100) |
| `tone` (banned words, verdicts, forecasts in our voice) | A1 copy (G1) | 0 | — | — | — | Awaiting labels (0 of 100) |

<!-- rates:end -->
