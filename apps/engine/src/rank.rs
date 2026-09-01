//! Ranking seams (A28/A29/A31): the total-order comparator, the Reranker and
//! ExplorationPolicy traits, and the seeded-fraction exploration policy with
//! propensity records. A11's fusion and A13's taste update route ordering and
//! randomness only through this module and `det`.

use crate::config::EngineConfig;
use crate::det::DetRng;
use crate::model::Listing;
use serde::Serialize;
use std::cmp::Ordering;

/// NaN is treated as the worst possible score: it can never panic a sort and
/// never floats to the top of a feed.
fn de_nan(s: f64) -> f64 {
    if s.is_nan() {
        f64::NEG_INFINITY
    } else {
        s
    }
}

/// The one ranked comparator: score descending, then id ascending. Total
/// order, so equal scores tie-break identically on every platform and run.
pub fn rank_cmp(score_a: f64, id_a: &str, score_b: f64, id_b: &str) -> Ordering {
    de_nan(score_b)
        .total_cmp(&de_nan(score_a))
        .then_with(|| id_a.cmp(id_b))
}

/// Ascending numeric sort with the id tie-break (price sorts and the like).
pub fn asc_cmp(v_a: f64, id_a: &str, v_b: f64, id_b: &str) -> Ordering {
    de_nan(v_a)
        .total_cmp(&de_nan(v_b))
        .then_with(|| id_a.cmp(id_b))
}

/// Context handed to rerankers. Grows in A11/A13 (taste vector, graph, index);
/// the identity impl reads nothing from it yet, hence the allow.
pub struct RankCtx<'a> {
    #[allow(dead_code)]
    pub cfg: &'a EngineConfig,
}

/// A31 seam: a learned re-rank head drops in here later (A24) without touching
/// handlers. Implementations must be deterministic for fixed weights; the
/// identity impl is the launch posture (engine decision D-07).
pub trait Reranker {
    fn rerank(&self, rows: &mut Vec<Listing>, ctx: &RankCtx);
}

pub struct IdentityReranker;

impl Reranker for IdentityReranker {
    fn rerank(&self, _rows: &mut Vec<Listing>, _ctx: &RankCtx) {}
}

/// One exploration placement, logged for off-policy evaluation (A29). Exploit
/// slots are not recorded; under the current policy their propensity is 1.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExploreRecord {
    pub slot: usize,
    pub item_id: String,
    /// Probability this item was drawn for this slot given the pool at draw
    /// time: 1 / pool_size_at_draw.
    pub propensity: f64,
}

/// Anneal exploration explore_max -> explore_min linearly over the user's
/// first explore_anneal_n interactions (feed/rerank.py explore_ratio_for).
pub fn explore_ratio_for(n_interactions: i64, cfg: &EngineConfig) -> f64 {
    if n_interactions >= cfg.explore_anneal_n {
        return cfg.explore_min;
    }
    let span = cfg.explore_max - cfg.explore_min;
    cfg.explore_max - span * (n_interactions.max(0) as f64 / cfg.explore_anneal_n as f64)
}

/// A31 seam: exploration policy. Seeded fraction now; a deterministic UCB
/// variant can replace it later without touching handlers. `ratio` is the
/// annealed exploration fraction for this user (explore_ratio_for).
pub trait ExplorationPolicy {
    fn mix(
        &self,
        ranked: Vec<Listing>,
        ratio: f64,
        cfg: &EngineConfig,
        rng: &mut DetRng,
    ) -> (Vec<Listing>, Vec<ExploreRecord>);
}

/// Every k-th slot (k = round(1/ratio), floor 2) is filled from the tail
/// beyond `explore_pool_start`, drawn without replacement by the seeded RNG.
/// Same (user, day, state, config) always yields the same slots and the same
/// draws; items above pool_start are never displaced.
pub struct SeededFraction;

impl ExplorationPolicy for SeededFraction {
    fn mix(
        &self,
        ranked: Vec<Listing>,
        ratio: f64,
        cfg: &EngineConfig,
        rng: &mut DetRng,
    ) -> (Vec<Listing>, Vec<ExploreRecord>) {
        let n = ranked.len();
        if ratio <= 0.0 || n <= cfg.explore_pool_start {
            return (ranked, Vec::new());
        }
        let k = ((1.0 / ratio).round() as usize).max(2);

        // Pool of ranked indices eligible for exploration draws.
        let mut pool: Vec<usize> = (cfg.explore_pool_start..n).collect();
        let mut taken = vec![false; n];
        let mut placements: Vec<Option<usize>> = vec![None; n];
        let mut records = Vec::new();

        for slot in 0..n {
            if (slot + 1) % k == 0 && !pool.is_empty() {
                let pool_size = pool.len();
                let idx = pool.swap_remove(rng.below(pool_size));
                taken[idx] = true;
                records.push(ExploreRecord {
                    slot,
                    item_id: ranked[idx].id.clone(),
                    propensity: 1.0 / pool_size as f64,
                });
                placements[slot] = Some(idx);
            }
        }

        let mut out: Vec<Listing> = Vec::with_capacity(n);
        let mut next = 0usize;
        for slot in 0..n {
            match placements[slot] {
                Some(idx) => out.push(ranked[idx].clone()),
                None => {
                    while next < n && taken[next] {
                        next += 1;
                    }
                    if next < n {
                        taken[next] = true;
                        out.push(ranked[next].clone());
                        next += 1;
                    }
                }
            }
        }
        (out, records)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explore_ratio_anneals() {
        let cfg = crate::config::EngineConfig::default();
        assert!((explore_ratio_for(0, &cfg) - 0.30).abs() < 1e-9);
        assert!((explore_ratio_for(25, &cfg) - 0.20).abs() < 1e-9);
        assert!((explore_ratio_for(50, &cfg) - 0.10).abs() < 1e-9);
        assert!((explore_ratio_for(500, &cfg) - 0.10).abs() < 1e-9);
    }

    #[test]
    fn rank_cmp_is_total_and_nan_sinks() {
        assert_eq!(rank_cmp(0.9, "b", 0.1, "a"), Ordering::Less); // higher score first
        assert_eq!(rank_cmp(0.5, "a", 0.5, "b"), Ordering::Less); // tie: id asc
        assert_eq!(rank_cmp(f64::NAN, "a", 0.0, "b"), Ordering::Greater); // NaN last
        assert_eq!(rank_cmp(f64::NAN, "a", f64::NAN, "b"), Ordering::Less); // NaN tie: id asc
    }

    #[test]
    fn no_forbidden_tokens_in_ranked_paths() {
        // The A28 gate: no ambient randomness, unstable hashing, or wall-clock
        // reads anywhere ranking logic lives. main.rs is exempt only for the
        // dispatch-time day_epoch default.
        // Tokens are assembled at runtime so this test's own source does not
        // trip the scan (rank.rs is scanned too).
        let forbidden: Vec<String> = [("rand", "::"), ("Default", "Hasher"), ("System", "Time"), ("thread_", "rng")]
            .iter()
            .map(|(a, b)| format!("{a}{b}"))
            .collect();
        for f in [
            "rank.rs", "handlers.rs", "det.rs", "config.rs", "model.rs",
            "scoring.rs", "taste.rs", "retrieval.rs", "embed.rs", "store.rs",
            "eval.rs", "evalrun.rs",
        ] {
            let src = std::fs::read_to_string(format!("{}/src/{}", env!("CARGO_MANIFEST_DIR"), f))
                .expect("source readable");
            for tok in &forbidden {
                assert!(!src.contains(tok.as_str()), "{f} contains forbidden token {tok}");
            }
        }
    }
}
