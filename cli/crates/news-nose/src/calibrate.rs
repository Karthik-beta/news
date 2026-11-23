//! Ranking calibration (Bureau §4.7, ADR 0012): a Bradley–Terry fit of the six weights of S to Karthik's
//! pairwise labels. Pure; the database side is `huddle`.
//!
//! `P(a ≻ b) = σ(ΔS / τ)` with `ΔS = 100 · (w · (z_a − z_b))` and `z_k = Q · x_k` (Q multiplies the whole
//! sum in S, so it belongs to the features). The fit maximises the mean log-likelihood of the labels, minus
//! an L2 pull towards the current weights, by projected gradient ascent onto the set the rules allow:
//! every weight ≥ 0, the six summing to 1, each within `max_step` of its current value.

use crate::config::Weights;
use crate::score::Components;

/// The weight order: I, M, F, D, N, G.
pub const NAMES: [&str; 6] = ["I", "M", "F", "D", "N", "G"];
pub type Vec6 = [f64; 6];

/// One S point of difference moves the odds by 1/τ in logit: 10 points of S is e : 1.
pub const TAU: f64 = 10.0;
/// The most a weight moves in one version (Bureau §4.7).
pub const MAX_STEP: f64 = 0.05;
/// Fewer labels than this say nothing.
pub const MIN_LABELS: usize = 10;

pub fn to_vec(w: &Weights) -> Vec6 {
    [w.i, w.m, w.f, w.d, w.n, w.g]
}

pub fn to_weights(v: Vec6) -> Weights {
    Weights {
        i: v[0],
        m: v[1],
        f: v[2],
        d: v[3],
        n: v[4],
        g: v[5],
    }
}

/// The features of a story: `Q · (I, M, F, D, N, G)`, so that `S = 100 · w · z`.
pub fn features(c: &Components) -> Vec6 {
    [c.i, c.m, c.f, c.d, c.n, c.g].map(|x| c.q * x)
}

pub struct Params {
    pub max_step: f64,
    /// Strength of the pull towards the current weights.
    pub shrink: f64,
    pub iterations: usize,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            max_step: MAX_STEP,
            shrink: 0.5,
            iterations: 5000,
        }
    }
}

/// Euclidean projection of `v` onto `{w : lo ≤ w ≤ hi, Σ w = 1}`: `w_k = clamp(v_k − μ, lo_k, hi_k)`
/// with μ found by bisection (the sum is non-increasing in μ). Needs `Σ lo ≤ 1 ≤ Σ hi`.
fn project(v: Vec6, lo: Vec6, hi: Vec6) -> Vec6 {
    let at = |mu: f64| -> Vec6 { std::array::from_fn(|k| (v[k] - mu).clamp(lo[k], hi[k])) };
    let (mut a, mut b) = (-2.0, 2.0);
    for _ in 0..100 {
        let mid = (a + b) / 2.0;
        if at(mid).iter().sum::<f64>() > 1.0 {
            a = mid;
        } else {
            b = mid;
        }
    }
    at((a + b) / 2.0)
}

fn sigmoid(x: f64) -> f64 {
    1.0 / (1.0 + (-x).exp())
}

fn dot(a: &Vec6, b: &Vec6) -> f64 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// Fit weights near `current` to the pairs; each pair is `z_winner − z_loser`.
pub fn fit(current: Vec6, diffs: &[Vec6], p: &Params) -> Vec6 {
    let lo = current.map(|c| (c - p.max_step).max(0.0));
    let hi = current.map(|c| (c + p.max_step).min(1.0));
    let beta = 100.0 / TAU;
    let n = diffs.len().max(1) as f64;
    let mean_sq = diffs.iter().map(|d| dot(d, d)).sum::<f64>() / n;
    let step = 1.0 / (beta * beta / 4.0 * mean_sq + p.shrink).max(1e-9);
    let mut w = current;
    for _ in 0..p.iterations {
        let mut g: Vec6 = [0.0; 6];
        for d in diffs {
            let miss = 1.0 - sigmoid(beta * dot(&w, d));
            for k in 0..6 {
                g[k] += miss * beta * d[k] / n;
            }
        }
        let next: Vec6 =
            std::array::from_fn(|k| w[k] + step * (g[k] - p.shrink * (w[k] - current[k])));
        w = project(next, lo, hi);
    }
    w
}

/// Kendall's τ between the weights' order and the labels: (agree − disagree) / pairs. `None` with no pairs.
pub fn agreement(w: &Vec6, diffs: &[Vec6]) -> Option<f64> {
    if diffs.is_empty() {
        return None;
    }
    let mut score = 0i32;
    for d in diffs {
        let s = dot(w, d);
        score += i32::from(s > 1e-12) - i32::from(s < -1e-12);
    }
    Some(f64::from(score) / diffs.len() as f64)
}

/// The rules a stored version must keep against its parent. Err names the first broken one.
pub fn check(parent: &Vec6, w: &Vec6) -> Result<(), String> {
    const EPS: f64 = 1e-9;
    if (w.iter().sum::<f64>() - 1.0).abs() > EPS {
        return Err("the weights do not sum to 1".into());
    }
    for k in 0..6 {
        if w[k] < -EPS {
            return Err(format!("weight {} is negative", NAMES[k]));
        }
        if (w[k] - parent[k]).abs() > MAX_STEP + EPS {
            return Err(format!("weight {} moves more than {MAX_STEP}", NAMES[k]));
        }
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    const V1: Vec6 = [0.30, 0.20, 0.15, 0.15, 0.10, 0.10];

    /// A deterministic generator in [0, 1): enough for synthetic labels, no dependency.
    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self) -> f64 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (self.0 >> 11) as f64 / (1u64 << 53) as f64
        }
    }

    /// Labels drawn from the model itself with the true weights: z in [0, 1]⁶, the winner by `P(a ≻ b)`.
    fn synthetic(truth: &Vec6, n: usize) -> Vec<Vec6> {
        let mut r = Lcg(42);
        (0..n)
            .map(|_| {
                let a: Vec6 = std::array::from_fn(|_| r.next());
                let b: Vec6 = std::array::from_fn(|_| r.next());
                let d: Vec6 = std::array::from_fn(|k| a[k] - b[k]);
                let p = sigmoid(100.0 / TAU * dot(truth, &d));
                if r.next() < p { d } else { d.map(|x| -x) }
            })
            .collect()
    }

    #[test]
    fn the_fit_recovers_known_weights_from_synthetic_labels() {
        let truth = [0.40, 0.10, 0.20, 0.05, 0.15, 0.10];
        let free = Params {
            max_step: 1.0,
            shrink: 0.0,
            iterations: 2000,
        };
        let w = fit(V1, &synthetic(&truth, 2500), &free);
        for k in 0..6 {
            assert!((w[k] - truth[k]).abs() < 0.05, "{}: {w:?}", NAMES[k]);
        }
        assert!((w.iter().sum::<f64>() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn a_week_moves_no_weight_more_than_the_step_and_keeps_the_sum() {
        // Labels that want a very different mix: the fit hits the ±0.05 box.
        let truth = [0.80, 0.02, 0.02, 0.02, 0.02, 0.12];
        let w = fit(V1, &synthetic(&truth, 500), &Params::default());
        check(&V1, &w).unwrap();
        assert!((w[0] - 0.35).abs() < 1e-6, "I should sit at the cap: {w:?}");
        assert!(w.iter().all(|x| *x >= 0.0));
    }

    #[test]
    fn weights_never_go_below_zero() {
        let near_zero = [0.50, 0.30, 0.15, 0.03, 0.01, 0.01];
        // Labels that want D, N and G gone.
        let truth = [0.6, 0.4, 0.0, 0.0, 0.0, 0.0];
        let w = fit(near_zero, &synthetic(&truth, 500), &Params::default());
        check(&near_zero, &w).unwrap();
        assert!(w[3] >= 0.0 && w[4] >= 0.0 && w[5] >= 0.0, "{w:?}");
    }

    #[test]
    fn check_refuses_a_big_move_a_bad_sum_and_a_negative() {
        assert!(check(&V1, &[0.40, 0.10, 0.15, 0.15, 0.10, 0.10]).is_err());
        assert!(check(&V1, &[0.31, 0.20, 0.15, 0.15, 0.10, 0.10]).is_err());
        assert!(
            check(
                &[0.04, 0.2, 0.2, 0.2, 0.2, 0.16],
                &[-0.01, 0.25, 0.2, 0.2, 0.2, 0.16]
            )
            .is_err()
        );
        assert!(check(&V1, &[0.33, 0.18, 0.15, 0.15, 0.09, 0.10]).is_ok());
    }

    #[test]
    fn agreement_is_kendalls_tau_over_the_labels() {
        let d = [
            [1.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            [-1.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        ];
        assert_eq!(agreement(&V1, &d), Some(0.0));
        assert_eq!(agreement(&V1, &d[..1]), Some(1.0));
        assert_eq!(agreement(&V1, &[]), None);
    }

    #[test]
    fn q_scales_the_features_so_s_is_100_times_the_dot_product() {
        let c = Components {
            i: 0.9,
            e: 0.0,
            p: 0.0,
            t: 0.0,
            m: 0.5,
            f: 0.8,
            d: 1.0,
            n: 1.0,
            g: 0.2,
            q: 0.9,
        };
        let s = 100.0 * dot(&V1, &features(&c));
        let direct = 100.0
            * 0.9
            * (0.30 * 0.9 + 0.20 * 0.5 + 0.15 * 0.8 + 0.15 * 1.0 + 0.10 * 1.0 + 0.10 * 0.2);
        assert!((s - direct).abs() < 1e-9);
    }
}
