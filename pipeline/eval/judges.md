# A12 judges

One binary judge per failure mode (Bureau Design §9.1, A12 in §7). A judge is trusted in Audit only after its true-positive and true-negative rates against about 100 of Karthik's labels are recorded here. Until then Audit's `judges` step is skipped and reports rest on the deterministic metrics.

Rates come from code comparing the judge's `pass` with Karthik's label on the same outputs; they are never typed by hand.

| Judge (failure mode) | Judges | Karthik's labels | True-positive rate | True-negative rate | Status |
| --- | --- | --- | --- | --- | --- |
| `wrong_direction` | A1 copy (G1), A7 (G6) | — | — | — | Awaiting labels |
| `unsupported_claim` | A1 copy (G1), A7 (G6) | — | — | — | Awaiting labels |
| `attribution` (misattributed quote) | A1 copy (G1), A7 (G6) | — | — | — | Awaiting labels |
| `missing_context` | A1 copy (G1), A7 (G6) | — | — | — | Awaiting labels |
| `tone` (banned words, verdicts, forecasts in our voice) | A1 copy (G1) | — | — | — | Awaiting labels |
