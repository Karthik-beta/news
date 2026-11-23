# ADR 0011: Embedding model for Taster

- Status: proposed
- Date: 2026-11-23
- Issue: #15 (Phase 4, wave 4), the embedding spike; decision D6 in the Bureau design
- Context documents: Bureau Design §3.6 (Taster), §9.1 (G3); spike write-up `docs/spikes/embeddings.md`; code and raw runs `spikes/embeddings/`

## Context

Taster embeds the candidates that have no event key (title plus the first paragraph or so) and compares them with story centroids from the last 72 hours: cosine ≥ 0.82 with a shared entity joins, 0.72–0.82 is the grey zone (A4 or keep separate), below 0.72 is a new story, and ≥ 0.85 to a recent page marks an update. The design recommended bge-small-en-v1.5 on fastembed-rs, int8 "if the spike shows no material loss", and asked for measurements on the laptop's i5-8250U (4 cores, 8 threads, no AVX-512 VNNI). Migration 0004 already declares `embeddings.v halfvec(384)`.

## Options measured

fastembed `=7.8.0`, ONNX Runtime, one cold process per run, 3 runs each, median shown; texts of ~50 / ~150 / ~300 tokens in batches of 16. CPU load from other work was 15–38 % for these rows (runs at 58–100 % load were discarded and repeated; all raw runs are kept).

| Model | Dim | Download | Cold load | Peak memory | docs/s ~50 / ~150 / ~300 tok (4 threads) | Separation AUC | Join ≥ 0.82 precision / recall |
|---|---|---|---|---|---|---|---|
| **bge-small-en-v1.5** | 384 | 128 MB | 0.5 s | 488 MB | **32.8 / 11.3 / 5.7** (2 threads: 28.2 / 9.8 / 4.9) | **0.968** | 0.88 / 0.96 |
| bge-small-en-v1.5 int8 | 384 | 64 MB | 0.35 s | 502 MB | 25.1 / 9.3 / 4.8 | 0.968 | 0.88 / 0.96 |
| nomic-embed-text-v1.5 | 768 | 523 MB | 1.4 s | 1,439 MB | 6.2 / 2.4 / 1.3 | 0.970 | 0.83 / 1.00 |
| EmbeddingGemma-300M | 768 | 1,198 MB | 4.0 s | 963 MB | 5.8 / 2.3 / 1.3 | 0.946 | 0.69 / 1.00 |
| all-MiniLM-L6-v2 | 384 | 87 MB | 0.6 s | 379 MB | 48.8 / 15.3 / 13.0 (cut at 256 tok) | 0.935 | 1.00 / 0.38 |

Separation is on 45 fixture-based pairs that would reach the embedding step (24 same story, 21 different, most of them near misses), out of a 62-pair spike set labelled by rule. It is not G3.

## Decision

1. **Model: BAAI bge-small-en-v1.5, fp32**, through fastembed-rs on ONNX Runtime (`EmbeddingModel::BGESmallENV15`, files from `Xenova/bge-small-en-v1.5`). The `embeddings.model` value is `bge-small-en-v1.5`.
2. **Vector dimension 384**: `halfvec(384)` with the HNSW `halfvec_cosine_ops` index, as migration 0004 has it. No change.
3. **No int8.** On this CPU the int8 export is 16–27 % slower than fp32, uses the same memory and gives the same cosines (within 0.001); only the download halves.
4. **No query prefix** (bge needs none for symmetric similarity); text is the title, a blank line, then the first paragraph, cut at the model's 512 tokens.
5. **Runtime:** one `TextEmbedding` loaded once in the service with 2 intra-op threads, batches of 16; the model cache lives outside the repo (under the service's data directory) and weights are never committed. Expected cost: ~5 docs/s at ~300 tokens, so 2,000 unkeyed candidates a day take about 7 minutes of two cores.
6. **Thresholds stay config** (0.72 / 0.82 / 0.85 as starting values). Calibration on G3 decides them; this spike's set suggests the bge join threshold will rise toward 0.88–0.90 to reach precision 0.97, widening the grey zone and raising A4 volume.
7. **Step (5), update detection:** a cosine match never overrides a *different* event key. When both the candidate and the page have event keys, only key equality makes an update; the cosine ≥ 0.85 rule applies only when one side has no key. Same-series releases a month apart score 0.88–1.00 with every model.

## Why

- bge-small ties nomic on separation at 4–5× the speed and a third of the memory, and matches the existing 384-dimension column. Gemma is slower, larger, gated by the Gemma licence and separates worse; MiniLM is faster but loses a fifth of same-story pairs below the 0.72 floor and truncates at 256 tokens.
- Near misses that share an entity (two Bank of England committee minutes in the same month, two speeches by one ECB board member) are what limit precision for every model; the shared-entity rule does not catch them. Identical text for different events (two "maintains the policy rate at 2¼%" decisions) scores 1.000 for every model, so event keys and dates must stay ahead of embeddings, as the design already orders them.

## Consequences

- Taster adds `fastembed` (default features minus image models) to the `cli/` workspace. ort's `download-binaries` feature fetches the ONNX Runtime binary at build time; CI never runs the model (G3 runs on stored fixture embeddings, as #15 requires), but it still compiles it.
- The release binary grows by roughly 25 MB (the spike binary is 28 MB) and the service by about 0.5 GB of memory while Taster is embedding.
- Bureau §3.6 named `bge-small-en-v1.5@int8` as the model value; that is replaced by `bge-small-en-v1.5`.
- Revisit if G3 precision cannot reach 0.97 at a recall Karthik accepts; nomic v1.5 (768 dimensions, a new column) is the measured fallback.
