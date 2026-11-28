# news-rewrite

Rewrite, revision handling (Bureau Design §3.11, §8.9). Keeps published pages correct when a source revises or restates data.

| Module | What it does |
| --- | --- |
| `find` | Published pages whose `page_refs` name a data point that a later vintage supersedes (the newest in the `supersedes` chain), and the derived points built from one, recomputed by `news_ledger::guard::derive` (unit, currency, scale, seasonal adjustment and comparable periods are checked). A refusal, or a new unit, currency, scale or seasonal adjustment on a point or a derived point, is a **direction flip**: the AI rewrites it. `apply::store_derived` writes the new `derived_points` rows before `apply`. A new vintage with the same text is not a change. |
| `classify` | Per page: **value only**, **restatement** (the old point was an earnings fact, so it has a fiscal period) or **direction flip** (a value or derived change crosses zero, or a guard now refuses the derived change). A flip beats a restatement. |
| `plan` | Classifies every change; `BULK` (100 pages); `budget` (Edition step 4: plan 100, defer the rest); `due_at_publish` (Edition step 10: only the code revisions planned at step 4). |
| `patch`, `apply` | The re-render by code: `data.json` re-keyed to the new `dp_` ids with the new values and `supersedes`, the stored Copy's refs renamed, `page.json` at version n+1, and for a restatement the "restated" `Note` (no digits; skipped when the page already has two notes). No AI, no typed number. Idempotent. |
| `update` | The `ai.write` job (A1, update mode) for a flip, keyed by page, version and diff; `news update <page>`'s output. |

`news update <page>` is a read: it shows the classification, the diff and the changed refs. Rendering through Plate, `news check` and publishing through Press are the steps after `apply`, run by Followup (and Edition step 10). The `page.revise` job handler still fails with class `bug` ("not built yet"): it needs the Bureau service's context.

Not checked in code: a context line ("lowest since …") that stops being true, and C5. Proof's C5 blocks such a page after the re-render.
