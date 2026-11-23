//! Embedding spike for Taster (issue #15). One process per measurement, so load time is a cold start.
//!
//!   embeddings-spike run <model> <threads> <cache_dir>   -> one JSON line on stdout
//!   embeddings-spike summarise <results.jsonl>            -> markdown tables on stdout
//!
//! Models: bge, bge-q, nomic, gemma, minilm. threads 0 = ONNX Runtime default.

use fastembed::{EmbeddingModel, TextEmbedding, TextInitOptions};
use serde::Deserialize;
use serde_json::{Value, json};
use std::time::Instant;

#[derive(Deserialize)]
struct Pair {
    id: String,
    same: bool,
    #[serde(default)]
    keyed: bool,
    a: String,
    b: String,
}

#[derive(Deserialize)]
struct Pairs {
    pairs: Vec<Pair>,
}

/// (fastembed model, query prefix the model card asks for in clustering / similarity tasks)
fn model(key: &str) -> (EmbeddingModel, &'static str) {
    match key {
        "bge" => (EmbeddingModel::BGESmallENV15, ""),
        "bge-q" => (EmbeddingModel::BGESmallENV15Q, ""),
        "nomic" => (EmbeddingModel::NomicEmbedTextV15, "clustering: "),
        "gemma" => (
            EmbeddingModel::EmbeddingGemma300M,
            "task: clustering | query: ",
        ),
        "minilm" => (EmbeddingModel::AllMiniLML6V2, ""),
        _ => panic!("unknown model {key}"),
    }
}

fn pairs() -> Vec<Pair> {
    let p: Pairs = serde_json::from_str(include_str!("../pairs.json")).expect("pairs.json");
    p.pairs
}

/// Deterministic candidate-like docs of ~`words` words, cut from the pair texts.
fn corpus(words: usize, n: usize) -> Vec<String> {
    let ps = pairs();
    let pool: Vec<&str> = ps
        .iter()
        .flat_map(|p| [p.a.as_str(), p.b.as_str()])
        .flat_map(|t| t.split_whitespace())
        .collect();
    (0..n)
        .map(|i| {
            let start = (i * 37) % pool.len();
            (0..words)
                .map(|k| pool[(start + k) % pool.len()])
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect()
}

fn cos(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    dot / (na * nb)
}

#[cfg(windows)]
fn peak_working_set_mb() -> f64 {
    #[repr(C)]
    #[derive(Default)]
    struct Counters {
        cb: u32,
        page_faults: u32,
        peak_ws: usize,
        ws: usize,
        a: usize,
        b: usize,
        c: usize,
        d: usize,
        pagefile: usize,
        peak_pagefile: usize,
    }
    unsafe extern "system" {
        fn GetCurrentProcess() -> isize;
        fn K32GetProcessMemoryInfo(h: isize, c: *mut Counters, cb: u32) -> i32;
    }
    let mut c = Counters {
        cb: size_of::<Counters>() as u32,
        ..Default::default()
    };
    // SAFETY: plain Win32 call with a correctly sized, owned struct.
    let ok = unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut c, c.cb) };
    assert!(ok != 0, "GetProcessMemoryInfo failed");
    c.peak_ws as f64 / 1_048_576.0
}

fn run(key: &str, threads: usize, cache: &str) {
    let (m, prefix) = model(key);
    let t0 = Instant::now();
    let mut opts = TextInitOptions::new(m).with_cache_dir(cache.into());
    if threads > 0 {
        opts = opts.with_intra_threads(threads);
    }
    let mut te = TextEmbedding::try_new(opts).expect("load model");
    let load_ms = t0.elapsed().as_secs_f64() * 1e3;
    let t1 = Instant::now();
    let first = te
        .embed(
            vec![format!("{prefix}Bank of Canada raises the policy rate")],
            None,
        )
        .unwrap();
    let first_ms = t1.elapsed().as_secs_f64() * 1e3;
    let dim = first[0].len();

    let mut buckets = vec![];
    for words in [40usize, 120, 230] {
        let docs: Vec<String> = corpus(words, 96)
            .into_iter()
            .map(|d| format!("{prefix}{d}"))
            .collect();
        let toks: usize = docs
            .iter()
            .map(|d| te.tokenizer.encode(d.as_str(), true).unwrap().len())
            .sum();
        let t = Instant::now();
        let out = te.embed(&docs, Some(16)).unwrap();
        let secs = t.elapsed().as_secs_f64();
        assert_eq!(out.len(), docs.len());
        buckets.push(
            json!({"words": words, "avg_tokens": toks as f64 / docs.len() as f64,
            "docs": docs.len(), "secs": secs, "docs_per_s": docs.len() as f64 / secs}),
        );
    }

    let ps = pairs();
    let texts: Vec<String> = ps
        .iter()
        .flat_map(|p| [format!("{prefix}{}", p.a), format!("{prefix}{}", p.b)])
        .collect();
    let e = te.embed(&texts, Some(16)).unwrap();
    let cosines: Vec<Value> = ps
        .iter()
        .enumerate()
        .map(|(i, p)| json!({"id": p.id, "same": p.same, "keyed": p.keyed, "cos": cos(&e[2 * i], &e[2 * i + 1])}))
        .collect();

    println!(
        "{}",
        json!({"model": key, "threads": threads, "dim": dim, "load_ms": load_ms, "first_embed_ms": first_ms,
            "peak_ws_mb": peak_working_set_mb(), "buckets": buckets, "pairs": cosines})
    );
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

fn summarise(path: &str) {
    let rows: Vec<Value> = std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter(|l| l.starts_with('{'))
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let mut keys: Vec<(String, u64)> = vec![];
    for r in &rows {
        let k = (
            r["model"].as_str().unwrap().to_string(),
            r["threads"].as_u64().unwrap(),
        );
        if !keys.contains(&k) {
            keys.push(k);
        }
    }
    println!(
        "| model | threads | runs | cpu load % (median before run) | load ms | first embed ms | peak WS MB | docs/s ~50 tok | docs/s ~150 tok | docs/s ~300 tok |"
    );
    println!("|---|---|---|---|---|---|---|---|---|---|");
    for (m, t) in &keys {
        let rs: Vec<&Value> = rows
            .iter()
            .filter(|r| r["model"] == m.as_str() && r["threads"] == *t)
            .collect();
        let f = |p: &dyn Fn(&Value) -> f64| median(rs.iter().map(|r| p(r)).collect());
        let b = |i: usize| f(&|r: &Value| r["buckets"][i]["docs_per_s"].as_f64().unwrap());
        let tok = |i: usize| rs[0]["buckets"][i]["avg_tokens"].as_f64().unwrap();
        println!(
            "| {m} | {t} | {} | {:.0} | {:.0} | {:.0} | {:.0} | {:.1} ({:.0} tok) | {:.1} ({:.0} tok) | {:.1} ({:.0} tok) |",
            rs.len(),
            f(&|r: &Value| r["cpu_load"].as_f64().unwrap_or(f64::NAN)),
            f(&|r: &Value| r["load_ms"].as_f64().unwrap()),
            f(&|r: &Value| r["first_embed_ms"].as_f64().unwrap()),
            f(&|r: &Value| r["peak_ws_mb"].as_f64().unwrap()),
            b(0),
            tok(0),
            b(1),
            tok(1),
            b(2),
            tok(2)
        );
    }

    // Cosines are deterministic per model; take the first run of each model.
    println!(
        "\n| model | subset | same: min / median / max | different: min / median / max | join ≥0.82: TP / FP / precision / recall | grey 0.72–0.82: same / diff | new <0.72: same / diff | update ≥0.85: same / diff | zero-false-join threshold | AUC |"
    );
    println!("|---|---|---|---|---|---|---|---|---|---|");
    let mut seen = vec![];
    for r in &rows {
        let m = r["model"].as_str().unwrap();
        if seen.contains(&m) {
            continue;
        }
        seen.push(m);
        for (label, keep) in [("all 62", false), ("embedding path (no keyed pairs)", true)] {
            let kept: Vec<&Value> = r["pairs"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|p| !(keep && p["keyed"].as_bool().unwrap()))
                .collect();
            let ps: Vec<(bool, f64)> = kept
                .iter()
                .map(|p| (p["same"].as_bool().unwrap(), p["cos"].as_f64().unwrap()))
                .collect();
            if keep {
                let ids = |f: &dyn Fn(&&&Value) -> bool| {
                    kept.iter()
                        .filter(f)
                        .map(|p| {
                            format!(
                                "{} {:.3}",
                                p["id"].as_str().unwrap(),
                                p["cos"].as_f64().unwrap()
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(", ")
                };
                eprintln!(
                    "{m}: different ≥0.82: [{}]; same <0.82: [{}]",
                    ids(&|p| !p["same"].as_bool().unwrap() && p["cos"].as_f64().unwrap() >= 0.82),
                    ids(&|p| p["same"].as_bool().unwrap() && p["cos"].as_f64().unwrap() < 0.82)
                );
            }
            let s: Vec<f64> = ps.iter().filter(|p| p.0).map(|p| p.1).collect();
            let d: Vec<f64> = ps.iter().filter(|p| !p.0).map(|p| p.1).collect();
            let mm = |v: &[f64]| {
                let mut v = v.to_vec();
                v.sort_by(|a, b| a.partial_cmp(b).unwrap());
                format!(
                    "{:.3} / {:.3} / {:.3}",
                    v[0],
                    v[v.len() / 2],
                    v[v.len() - 1]
                )
            };
            let cnt =
                |v: &[f64], lo: f64, hi: f64| v.iter().filter(|c| **c >= lo && **c < hi).count();
            let (tp, fp) = (cnt(&s, 0.82, 9.0), cnt(&d, 0.82, 9.0));
            // Lowest threshold with zero false joins, and the recall it keeps.
            let max_d = d.iter().cloned().fold(f64::MIN, f64::max);
            // Threshold-free: the chance a same pair scores above a different pair.
            let auc = s
                .iter()
                .flat_map(|a| {
                    d.iter().map(move |b| {
                        if a > b {
                            1.0
                        } else if a == b {
                            0.5
                        } else {
                            0.0
                        }
                    })
                })
                .sum::<f64>()
                / (s.len() * d.len()) as f64;
            let rec = s.iter().filter(|c| **c > max_d).count() as f64 / s.len() as f64;
            println!(
                "| {m} | {label} | {} | {} | {tp} / {fp} / {:.2} / {:.2} | {} / {} | {} / {} | {} / {} | >{max_d:.3} keeps recall {rec:.2} | {auc:.3} |",
                mm(&s),
                mm(&d),
                tp as f64 / (tp + fp).max(1) as f64,
                tp as f64 / s.len() as f64,
                cnt(&s, 0.72, 0.82),
                cnt(&d, 0.72, 0.82),
                cnt(&s, -9.0, 0.72),
                cnt(&d, -9.0, 0.72),
                cnt(&s, 0.85, 9.0),
                cnt(&d, 0.85, 9.0),
            );
        }
    }
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    match a.get(1).map(String::as_str) {
        Some("run") => run(&a[2], a[3].parse().unwrap(), &a[4]),
        Some("summarise") => summarise(&a[2]),
        // Fetch into the cache (and time it) without benchmarking.
        Some("load") => {
            let t = Instant::now();
            TextEmbedding::try_new(
                TextInitOptions::new(model(&a[2]).0).with_cache_dir(a[3].clone().into()),
            )
            .expect("load model");
            println!("{} loaded in {:.1} s", a[2], t.elapsed().as_secs_f64());
        }
        _ => eprintln!(
            "usage: run <model> <threads> <cache_dir> | load <model> <cache_dir> | summarise <results.jsonl>"
        ),
    }
}
