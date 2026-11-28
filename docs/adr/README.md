# Architecture decision records

Each record settles one technical question for News, with the context, the options considered, measurements where there are any, the decision and its consequences. Records are numbered in order and never renumbered. When a decision changes, add a new record that supersedes the old one, and mark the old one's status "superseded by ADR NNNN".

File names: `NNNN-short-title.md`. Status is one of proposed, accepted, superseded.

| ADR | Title | Status | Date |
|---|---|---|---|
| [0001](0001-openui-server-rendering.md) | Server rendering of OpenUI pages (and the component index token budget) | accepted | 2026-11-15 |
| [0010](0010-ssr-routing-and-caching.md) | SSR site routing, data adapter and edge caching | proposed | 2026-11-22 |
| [0011](0011-embedding-model.md) | Embedding model for Taster (bge-small-en-v1.5, 384 dimensions) | proposed | 2026-11-23 |
| [0012](0012-scoring-and-selection.md) | Scoring and selection (Nose and Desk) | proposed | 2026-11-23 |
| [0013](0013-proof-and-templated-fallback.md) | Proof and the templated fallback | proposed | 2026-11-25 |
