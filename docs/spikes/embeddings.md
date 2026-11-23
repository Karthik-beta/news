# Spike: embedding model for Taster

The decision is [ADR 0011](../adr/0011-embedding-model.md). Code, pair set and raw runs: `spikes/embeddings/` (standalone Cargo project, not in the `cli/` workspace).

## Question

Which of bge-small-en-v1.5, nomic-embed-text-v1.5, EmbeddingGemma-300M and all-MiniLM-L6-v2, run through fastembed-rs on ONNX Runtime on the i5-8250U laptop, should Taster use (Bureau §3.6, decision D6)? How fast, how big, how much memory, and how well does cosine similarity separate same-story from different-story candidates at the design's 0.72 / 0.82 / 0.85 thresholds?

## What was built

- A ~230-line Rust binary on `fastembed =7.8.0` (default features minus image models). One process per measurement, so every load is a cold start from the local model cache; peak memory is the process's peak working set (`K32GetProcessMemoryInfo`).
- Throughput: 96 candidate-like texts per length bucket cut from the pair texts (~50, ~150 and ~300 tokens by each model's own tokenizer), embedded in batches of 16.
- Precision: 62 pairs from the recorded fixtures (central bank feeds, NASA, arXiv, Crossref, the SEC submissions fixture, Diary's BEA and FOMC calendars, Roster's G7 mention headlines). 28 are the same story, 34 are different, labelled by rule: same entity and same event (event key, or entity + event type + period) is the same story; another meeting, period, release or speech is different. Where a fixture has only one source for a story, the other side is a wire-style rewrite written for this spike from the fixture's facts only. 17 pairs have an event key on both sides, so Taster would never compare them by embedding; the "embedding path" rows leave them out (45 pairs: 24 same, 21 different). The set leans hard on near misses (same institution, same genre, different event). **It is not G3** (200 pairs Karthik confirms).
- Each model ran 3 times; tables show the median. CPU load (`\Processor(_Total)\% Processor Time`, 3 s before each run) is in the table because other agents were compiling Rust on the same laptop. The first pass of bge, bge-int8 and nomic at 4 threads ran at 58–100 % load and was 1.5–2.5× slower; they were re-run at 15–20 % load (`results-rerun.jsonl`), and those numbers are used. The ~15 % floor is a stray `find` process on one core.

## Results

### Speed, size, memory (4 intra-op threads unless noted; batch 16)

| Model | Dim | Download | Cold load | First embed | Peak memory | docs/s ~50 tok | docs/s ~150 tok | docs/s ~300 tok | CPU load |
|---|---|---|---|---|---|---|---|---|---|
| bge-small-en-v1.5 | 384 | 128 MB | 0.52 s | 12 ms | 488 MB | **32.8** | **11.3** | **5.7** | 19 % |
| bge-small-en-v1.5, 2 threads | 384 | | 0.57 s | 12 ms | 487 MB | 28.2 | 9.8 | 4.9 | 30 % |
| bge-small-en-v1.5 int8 | 384 | 64 MB | 0.35 s | 38 ms | 502 MB | 25.1 | 9.3 | 4.8 | 17 % |
| bge-small-en-v1.5 int8, 2 threads | 384 | | 0.36 s | 37 ms | 501 MB | 20.7 | 7.8 | 4.0 | 28 % |
| all-MiniLM-L6-v2 | 384 | 87 MB | 0.64 s | 12 ms | 379 MB | 48.8 | 15.3 | 13.0 (truncated at 256 tok) | 38 % |
| nomic-embed-text-v1.5 | 768 | 523 MB | 1.45 s | 48 ms | 1,439 MB | 6.2 | 2.4 | 1.3 | 16 % |
| EmbeddingGemma-300M | 768 | 1,198 MB | 4.0 s | 60 ms | 963 MB | 5.8 | 2.3 | 1.3 | 28 % |

First download times (home connection): bge 53 s, bge-int8 28 s, nomic 184 s, Gemma 507 s. The ONNX mirror of EmbeddingGemma (`onnx-community/embeddinggemma-300m-ONNX`) downloaded without a Hugging Face login; the Gemma licence terms still apply. The bge release binary is 28 MB with ONNX Runtime linked in.

### Same-story separation (cosine)

| Model | Subset | Same: min / median / max | Different: min / median / max | Join ≥ 0.82: TP / FP, precision, recall | Grey 0.72–0.82: same / diff | New < 0.72: same / diff | ≥ 0.85: same / diff | AUC |
|---|---|---|---|---|---|---|---|---|
| bge-small | all 62 | 0.732 / 0.908 / 0.971 | 0.325 / 0.791 / 1.000 | 25 / 15, 0.62, 0.89 | 3 / 5 | 0 / 14 | 23 / 13 | 0.737 |
| bge-small | embedding path (45) | 0.783 / 0.908 / 0.958 | 0.325 / 0.661 / 0.882 | 23 / 3, **0.88, 0.96** | 1 / 5 | 0 / 13 | 21 / 2 | **0.968** |
| MiniLM | embedding path | 0.577 / 0.813 / 0.925 | −0.040 / 0.431 / 0.819 | 9 / 0, 1.00, 0.38 | 10 / 3 | 5 / 18 | 6 / 0 | 0.935 |
| nomic v1.5 | embedding path | 0.854 / 0.952 / 0.977 | 0.540 / 0.790 / 0.940 | 24 / 5, 0.83, 1.00 | 0 / 8 | 0 / 8 | 24 / 4 | **0.970** |
| EmbeddingGemma | embedding path | 0.870 / 0.959 / 0.985 | 0.471 / 0.829 / 0.952 | 24 / 11, 0.69, 1.00 | 0 / 6 | 0 / 4 | 24 / 7 | 0.946 |

bge-int8 cosines match bge fp32 to within 0.001 on every pair, so its rows are identical. Precision and recall of a join at other thresholds, embedding path:

| Threshold | bge-small | nomic | Gemma | MiniLM |
|---|---|---|---|---|
| 0.82 | P 0.88, R 0.96 | P 0.83, R 1.00 | P 0.69, R 1.00 | P 1.00, R 0.38 |
| 0.85 | P 0.91, R 0.88 | P 0.86, R 1.00 | P 0.77, R 1.00 | P 1.00, R 0.25 |
| 0.88 | P 0.94, R 0.71 | P 0.88, R 0.92 | P 0.81, R 0.92 | P 1.00, R 0.17 |
| 0.90 | P 1.00, R 0.50 | P 0.91, R 0.88 | P 0.84, R 0.88 | P 1.00, R 0.12 |

What the thresholds do with bge-small:

- **0.72 (new story):** no same-story pair on the embedding path falls below it (lowest 0.783: an SEC 8-K cover line against a "results" headline); 13 of 21 different pairs do. It is a safe floor.
- **0.82 (join):** 3 false joins, all the same institution and genre but a different event: two Bank of England committee minutes for September (0.882), two Cipollone speeches (0.867), Schnabel and Lane speeches (0.826). Each pair shares a resolved entity (the Bank, the ECB), so the "≥ 1 shared entity" rule does not stop them. Precision 0.88 is below the 0.97 G3 target; on this set, zero false joins needs > 0.882 and keeps 71 % recall.
- **0.85 (update):** 2 of 21 different pairs reach it on the embedding path. On **keyed** pairs it is much worse: next month's release of the same series scores 0.88–1.00 (Personal Income September vs October 0.954, trade August vs September 0.950, two Bank of Canada holds with identical text 1.000). Step (5) reads "same event key, **or** cos ≥ 0.85 to a page from the last 30 days with a shared entity", so as written it would mark every monthly release an UPDATE of last month's page.
- **Identical text, different event** (two Bank of Canada holds, three "Federal Reserve issues FOMC statement" items): cosine 1.000 for every model. Only event keys and dates can separate these.

## Recommendation

**bge-small-en-v1.5, fp32, through fastembed-rs, 384 dimensions (`halfvec(384)`, as migration 0004 already has), 2 intra-op threads in the service.**

- It ties nomic for the best separation (AUC 0.968 vs 0.970) at 4–5× nomic's throughput and a third of its memory, with the smallest useful download. Gemma is 4–6× slower than bge, needs 9× the disk and separates worse; its similarities sit high (different-story median 0.83), so every threshold would move. MiniLM is the fastest but truncates at 256 tokens and puts a fifth of same-story pairs below 0.72.
- **Not int8.** The int8 export gives the same cosines but is 16–27 % *slower* on this CPU (no VNNI/AMX) and uses as much memory; it only halves the download.
- Throughput: ~5 docs/s at ~300 tokens on 2 threads, ~28 docs/s for title-only items. 2,000 unkeyed candidates a day of ~300 tokens is about 7 minutes of 2 cores; the design's 20–60 docs/s guess holds only for short texts.
- Thresholds stay config. Expect G3 to move the join threshold up toward ~0.88–0.90 for bge and widen the grey zone, which sends more pairs to A4. Fix step (5) so a cosine match never overrides a *different* event key.

## Risks

- The pair set is small, partly synthetic (the rewrites) and labelled by the spike agent; precision numbers are indicative only until G3.
- Throughput was measured with other agents running; the quiet-window runs had ~15–20 % background load, so the service will see similar or lower numbers when the laptop is busy.
- ort's `download-binaries` feature fetches the ONNX Runtime binary at build time; the `cli/` build will need it (or a vendored runtime) once Taster adds fastembed.
- Peak memory (~490 MB with batch 16 at 300 tokens) is mostly ONNX Runtime's arena; smaller batches would lower it.
