//! A26, part 1: pure evaluation math. Ranking metrics (recall@k, precision@k,
//! nDCG@k, MRR, average precision, coverage, Gini) ported from the fork's
//! `eval/metrics.py`, plus off-policy estimators (IPS, SNIPS, effective sample
//! size) that the fork does not have. The off-policy estimators are what the
//! A29 exploration propensity logs exist to feed: they estimate the value of a
//! candidate ranking policy from data logged under a different (exploration)
//! policy, with no new traffic.
//!
//! Everything here is pure and hand-verifiable; the goldens below are computed
//! by hand and asserted to 1e-6.

use crate::det::DetRng;
use serde::Serialize;
use std::collections::BTreeSet;

fn log2(x: f64) -> f64 {
    x.ln() / std::f64::consts::LN_2
}

// ---- ranking metrics (fork eval/metrics.py definitions) ----

/// Fraction of relevant items appearing in the top-k. 0.0 if none relevant.
pub fn recall_at_k(ranked: &[String], relevant: &BTreeSet<String>, k: usize) -> f64 {
    if relevant.is_empty() {
        return 0.0;
    }
    let hits = ranked.iter().take(k).filter(|id| relevant.contains(*id)).count();
    hits as f64 / relevant.len() as f64
}

/// Fraction of the top-k that is relevant. 0.0 if k == 0. Part of the metric
/// library (exercised in tests); not on the default report path.
#[allow(dead_code)]
pub fn precision_at_k(ranked: &[String], relevant: &BTreeSet<String>, k: usize) -> f64 {
    if k == 0 {
        return 0.0;
    }
    let hits = ranked.iter().take(k).filter(|id| relevant.contains(*id)).count();
    hits as f64 / k as f64
}

/// Binary nDCG@k: DCG@k / IDCG@k with rel in {0,1}, gain 1/log2(rank+1).
pub fn ndcg_at_k(ranked: &[String], relevant: &BTreeSet<String>, k: usize) -> f64 {
    if relevant.is_empty() {
        return 0.0;
    }
    let mut dcg = 0.0;
    for (i, id) in ranked.iter().take(k).enumerate() {
        if relevant.contains(id) {
            dcg += 1.0 / log2((i + 2) as f64); // rank = i+1, denom log2(rank+1)
        }
    }
    let ideal = relevant.len().min(k);
    let mut idcg = 0.0;
    for i in 0..ideal {
        idcg += 1.0 / log2((i + 2) as f64);
    }
    if idcg == 0.0 {
        0.0
    } else {
        dcg / idcg
    }
}

/// Reciprocal rank of the first relevant item; 0.0 if none present.
pub fn mrr(ranked: &[String], relevant: &BTreeSet<String>) -> f64 {
    for (i, id) in ranked.iter().enumerate() {
        if relevant.contains(id) {
            return 1.0 / (i + 1) as f64;
        }
    }
    0.0
}

/// Average precision: mean of precision@rank at each relevant hit, over the
/// number of relevant items.
pub fn average_precision(ranked: &[String], relevant: &BTreeSet<String>) -> f64 {
    if relevant.is_empty() {
        return 0.0;
    }
    let mut hits = 0usize;
    let mut acc = 0.0;
    for (i, id) in ranked.iter().enumerate() {
        if relevant.contains(id) {
            hits += 1;
            acc += hits as f64 / (i + 1) as f64;
        }
    }
    acc / relevant.len() as f64
}

/// Share of the catalog surfaced in any top-k across users.
pub fn coverage(all_ranked: &[Vec<String>], catalog: &BTreeSet<String>, k: usize) -> f64 {
    if catalog.is_empty() {
        return 0.0;
    }
    let mut surfaced: BTreeSet<&String> = BTreeSet::new();
    for ranked in all_ranked {
        for id in ranked.iter().take(k) {
            surfaced.insert(id);
        }
    }
    surfaced.iter().filter(|id| catalog.contains(**id)).count() as f64 / catalog.len() as f64
}

/// Gini concentration of top-k impressions across items (0 even, ->1 skewed).
/// Ports the fork's exact formula over ascending-sorted counts.
pub fn gini(all_ranked: &[Vec<String>], k: usize) -> f64 {
    use std::collections::BTreeMap;
    let mut counts: BTreeMap<&String, i64> = BTreeMap::new();
    for ranked in all_ranked {
        for id in ranked.iter().take(k) {
            *counts.entry(id).or_insert(0) += 1;
        }
    }
    let mut values: Vec<i64> = counts.into_values().collect();
    values.sort_unstable();
    let n = values.len();
    if n == 0 {
        return 0.0;
    }
    let total: i64 = values.iter().sum();
    if total == 0 {
        return 0.0;
    }
    let cumulative: i64 = values.iter().enumerate().map(|(i, v)| (i as i64 + 1) * v).sum();
    (2 * cumulative) as f64 / (n as i64 * total) as f64 - (n as f64 + 1.0) / n as f64
}

// ---- off-policy estimators (new; consume the A29 propensity logs) ----

/// Importance weight pi_e(a) / pi_0(a) for one logged action. A zero logging
/// propensity is a logging bug (the action could not have been drawn), so it
/// contributes weight 0 rather than dividing by zero.
pub fn importance_weight(target_prob: f64, logging_prob: f64) -> f64 {
    if logging_prob <= 0.0 {
        0.0
    } else {
        target_prob / logging_prob
    }
}

/// Inverse Propensity Scoring: mean over logged (weight, reward) of w*r.
/// Unbiased estimate of the target policy's value under the logs.
pub fn ips(samples: &[(f64, f64)]) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    samples.iter().map(|(w, r)| w * r).sum::<f64>() / samples.len() as f64
}

/// Self-Normalized IPS: sum(w*r)/sum(w). Lower variance than IPS, and scale-
/// free in the weights; 0 when all weights are 0.
pub fn snips(samples: &[(f64, f64)]) -> f64 {
    let wsum: f64 = samples.iter().map(|(w, _)| w).sum();
    if wsum <= 0.0 {
        return 0.0;
    }
    samples.iter().map(|(w, r)| w * r).sum::<f64>() / wsum
}

/// Effective sample size of a set of importance weights: (sum w)^2 / sum(w^2).
/// A low ESS relative to n flags an off-policy estimate resting on a handful of
/// high-weight logs, i.e. an untrustworthy number.
pub fn effective_sample_size(weights: &[f64]) -> f64 {
    let s1: f64 = weights.iter().sum();
    let s2: f64 = weights.iter().map(|w| w * w).sum();
    if s2 <= 0.0 {
        0.0
    } else {
        s1 * s1 / s2
    }
}

/// Numerically stable softmax over scores with temperature T (T > 0).
pub fn softmax(scores: &[f64], temperature: f64) -> Vec<f64> {
    if scores.is_empty() {
        return Vec::new();
    }
    let t = temperature.max(1e-9);
    let max = scores.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let exps: Vec<f64> = scores.iter().map(|s| ((s - max) / t).exp()).collect();
    let sum: f64 = exps.iter().sum();
    if sum <= 0.0 {
        return vec![1.0 / scores.len() as f64; scores.len()];
    }
    exps.iter().map(|e| e / sum).collect()
}

// ---- aggregation (fork eval/report.py) ----

#[derive(Clone, Serialize)]
pub struct MetricStat {
    pub mean: f64,
    pub ci_low: f64,
    pub ci_high: f64,
    pub n: usize,
}

fn percentile(sorted: &[f64], q: f64) -> f64 {
    match sorted.len() {
        0 => 0.0,
        1 => sorted[0],
        len => {
            let rank = q / 100.0 * (len - 1) as f64;
            let lo = rank.floor() as usize;
            let hi = rank.ceil() as usize;
            if lo == hi {
                sorted[lo]
            } else {
                let frac = rank - lo as f64;
                sorted[lo] * (1.0 - frac) + sorted[hi] * frac
            }
        }
    }
}

/// Per-value bootstrap 95% CI. Deterministic given the seed (DetRng), so the
/// whole eval report is reproducible and can be regression-gated.
pub fn bootstrap_ci(values: &[f64], seed: u64, resamples: usize) -> MetricStat {
    let n = values.len();
    if n == 0 {
        return MetricStat { mean: 0.0, ci_low: 0.0, ci_high: 0.0, n: 0 };
    }
    let mean = values.iter().sum::<f64>() / n as f64;
    let mut rng = DetRng::new(seed);
    let mut means: Vec<f64> = Vec::with_capacity(resamples);
    for _ in 0..resamples {
        let mut acc = 0.0;
        for _ in 0..n {
            acc += values[rng.below(n)];
        }
        means.push(acc / n as f64);
    }
    means.sort_by(f64::total_cmp);
    MetricStat {
        mean,
        ci_low: percentile(&means, 2.5),
        ci_high: percentile(&means, 97.5),
        n,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(xs: &[&str]) -> Vec<String> {
        xs.iter().map(|s| s.to_string()).collect()
    }
    fn set(xs: &[&str]) -> BTreeSet<String> {
        xs.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn ranking_metric_goldens() {
        let ranked = ids(&["a", "b", "c", "d", "e"]);
        let rel = set(&["b", "e"]);
        assert!((recall_at_k(&ranked, &rel, 3) - 0.5).abs() < 1e-12);
        assert!((precision_at_k(&ranked, &rel, 3) - 1.0 / 3.0).abs() < 1e-12);
        assert!((mrr(&ranked, &rel) - 0.5).abs() < 1e-12);
        // DCG = 1/log2(3) + 1/log2(6); IDCG = 1 + 1/log2(3)
        assert!((ndcg_at_k(&ranked, &rel, 5) - 0.6240506).abs() < 1e-5);
        assert!((average_precision(&ranked, &rel) - 0.45).abs() < 1e-12);
        // empty-relevant guards
        assert_eq!(recall_at_k(&ranked, &set(&[]), 3), 0.0);
        assert_eq!(mrr(&ranked, &set(&["z"])), 0.0);
    }

    #[test]
    fn coverage_and_gini_goldens() {
        let all = vec![ids(&["a", "b", "c"]), ids(&["b", "c", "d"])];
        let cat = set(&["a", "b", "c", "d", "e"]);
        assert!((coverage(&all, &cat, 2) - 0.6).abs() < 1e-12);
        // top-2 counts a:1 b:2 c:1 -> gini 0.16667
        assert!((gini(&all, 2) - 0.166_666_666_666).abs() < 1e-9);
    }

    #[test]
    fn off_policy_goldens() {
        let samples = [(2.0, 1.0), (0.0, 1.0), (1.0, 0.0), (4.0, 0.5)];
        assert!((ips(&samples) - 1.0).abs() < 1e-12);
        assert!((snips(&samples) - 4.0 / 7.0).abs() < 1e-12);
        let weights = [2.0, 0.0, 1.0, 4.0];
        assert!((effective_sample_size(&weights) - 49.0 / 21.0).abs() < 1e-12);
        assert_eq!(importance_weight(0.5, 0.0), 0.0);
        assert!((importance_weight(0.5, 0.25) - 2.0).abs() < 1e-12);
    }

    #[test]
    fn softmax_is_a_distribution_and_temperature_sharpens() {
        let p = softmax(&[1.0, 2.0, 3.0], 1.0);
        assert!((p.iter().sum::<f64>() - 1.0).abs() < 1e-12);
        assert!(p[2] > p[1] && p[1] > p[0]);
        // colder temperature concentrates mass on the argmax
        let cold = softmax(&[1.0, 2.0, 3.0], 0.25);
        assert!(cold[2] > p[2]);
    }

    #[test]
    fn bootstrap_is_deterministic_and_brackets_the_mean() {
        let vals: Vec<f64> = (0..40).map(|i| (i % 5) as f64 / 4.0).collect();
        let a = bootstrap_ci(&vals, 42, 500);
        let b = bootstrap_ci(&vals, 42, 500);
        assert_eq!(a.mean, b.mean);
        assert_eq!(a.ci_low, b.ci_low);
        assert_eq!(a.ci_high, b.ci_high);
        assert!(a.ci_low <= a.mean && a.mean <= a.ci_high);
        assert_eq!(a.n, 40);
    }
}
