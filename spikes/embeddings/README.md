# Embedding spike (issue #15)

Standalone Cargo project (its own `[workspace]` and `Cargo.lock`), so fastembed and ONNX Runtime stay out of the `cli/` build. Findings: `docs/spikes/embeddings.md`; decision: `docs/adr/0011-embedding-model.md`.

- `pairs.json`: 62 same-or-different pairs labelled by rule. **Not G3.**
- `src/main.rs`: `run <model> <threads> <cache>` prints one JSON line (cold load, first embed, docs/s at ~50/150/300 tokens, peak working set, pair cosines); `load <model> <cache>` only downloads; `summarise <file>` prints the tables.
- `run.ps1`: each model 3 times per thread count, CPU load sampled before each run.
- `results.jsonl`, `results-rerun.jsonl`: the raw runs behind the tables.

Re-run (models go to a cache outside the repo; never commit weights):

```powershell
cd spikes/embeddings
cargo build --release          # ort downloads its ONNX Runtime binary at build time
pwsh -File run.ps1 -Cache $env:LOCALAPPDATA\news\fastembed -Models bge,minilm -Threads 4,2 -Out my.jsonl
./target/release/embeddings-spike.exe summarise my.jsonl
```

Models: `bge` (bge-small-en-v1.5), `bge-q` (its int8 export), `nomic` (nomic-embed-text-v1.5, `clustering: ` prefix), `gemma` (EmbeddingGemma-300M, `task: clustering | query: ` prefix), `minilm` (all-MiniLM-L6-v2).
