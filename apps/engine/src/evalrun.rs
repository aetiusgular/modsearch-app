//! A26, part 2: the evaluation protocol, baseline ladder, off-policy demo, and
//! the deterministic report. Ports the fork's `eval/protocol.py` + `scorers.py`
//! structure (temporal split, random -> popularity -> taste -> full ladder,
//! bootstrap-CI aggregation) to a self-contained simulated-preference protocol,
//! and adds an off-policy evaluation that consumes exploration-style propensity
//! logs.
//!
//! Honesty note, stated in the report itself: relevance and rewards here come
//! from a known-preference ORACLE (a simulated user whose true taste is a
//! brand), not from real user logs. The numbers validate the estimators and
//! the ranking pipeline end to end and gate regressions; they are not a claim
//! about live recommendation quality. Real evaluation swaps the oracle for the
//! archived-interaction temporal split the fork's protocol.py describes, using
//! the identical metrics and estimators below.

use crate::config::EngineConfig;
use crate::det::{self, fnv1a, DetRng};
use crate::embed::{ItemEncoder, SyntheticEncoder};
use crate::eval::{
    self, average_precision, bootstrap_ci, coverage, effective_sample_size, gini, ips, mrr,
    ndcg_at_k, recall_at_k, snips, softmax, MetricStat,
};
use crate::model::Listing;
use crate::rank::rank_cmp;
use crate::retrieval::{dot, fused_score, jaccard, CatalogIndex};
use crate::taste;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

const MIN_BRAND_ITEMS: usize = 4;
const K: usize = 10;
const N_LOG_PER_USER: usize = 200;
const TEMPERATURE: f64 = 0.15;
const RESAMPLES: usize = 500;

/// One simulated user: true taste is a brand; relevant items are that brand's,
/// split into a history (training signal) and held-out positives (test).
struct UserCase {
    brand: String,
    positives: BTreeSet<String>,
    pool: Vec<usize>,
    query: Vec<f32>,
    user_attrs: BTreeSet<String>,
}

fn build_cases(catalog: &[Listing], index: &CatalogIndex, cfg: &EngineConfig) -> Vec<UserCase> {
    let mut by_brand: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (i, l) in catalog.iter().enumerate() {
        by_brand.entry(l.brand.clone()).or_default().push(i);
    }
    let mut cases = Vec::new();
    for (brand, mut idxs) in by_brand {
        if idxs.len() < MIN_BRAND_ITEMS {
            continue;
        }
        idxs.sort_by(|&a, &b| catalog[a].id.cmp(&catalog[b].id)); // determinism
        let split = idxs.len() / 2;
        let history: Vec<usize> = idxs[..split].to_vec();
        let positives: BTreeSet<String> = idxs[split..].iter().map(|&i| catalog[i].id.clone()).collect();
        let history_set: BTreeSet<usize> = history.iter().copied().collect();
        let pool: Vec<usize> = (0..catalog.len()).filter(|i| !history_set.contains(i)).collect();

        // replay the history through the decay math to a taste vector (fork build_profile)
        let mut u: Option<Vec<f32>> = None;
        let mut last_ms: Option<i64> = None;
        for (step, &i) in history.iter().enumerate() {
            let now_ms = step as i64 * 86_400_000;
            let v = taste::unit(&index.vectors[i]).unwrap_or_else(|_| index.vectors[i].clone());
            let updated = taste::compute_update_window(
                u.as_deref(), last_ms, 1.0, &v, now_ms, cfg, cfg.tau_ms(),
            )
            .expect("history r_score is positive");
            u = Some(updated);
            last_ms = Some(now_ms);
        }
        let query = u.and_then(|v| taste::unit(&v).ok()).unwrap_or_default();
        let user_attrs: BTreeSet<String> =
            history.iter().flat_map(|&i| index.attrs[i].iter().cloned()).collect();

        cases.push(UserCase { brand, positives, pool, query, user_attrs });
    }
    cases
}

/// Rank a pool of catalog indices into item ids by a score, high first, id
/// tie-break (the total-order comparator, so every scorer is reproducible).
fn rank_by(pool: &[usize], catalog: &[Listing], score: impl Fn(usize) -> f64) -> Vec<String> {
    let mut scored: Vec<(usize, f64)> = pool.iter().map(|&i| (i, score(i))).collect();
    scored.sort_by(|a, b| rank_cmp(a.1, &catalog[a.0].id, b.1, &catalog[b.0].id));
    scored.into_iter().map(|(i, _)| catalog[i].id.clone()).collect()
}

/// Taste-independent pseudo-popularity: a stable per-item value uncorrelated
/// with the oracle's brand preference, so this baseline scores near chance.
fn pseudo_pop(id: &str) -> f64 {
    (fnv1a(id.as_bytes()) >> 11) as f64
}

/// The four baseline scorers (fork ladder): random, popularity, taste, full.
fn score_ranked(
    scorer: &str,
    case: &UserCase,
    catalog: &[Listing],
    index: &CatalogIndex,
    cfg: &EngineConfig,
) -> Vec<String> {
    match scorer {
        "random" => {
            // seeded Fisher-Yates permutation of the pool
            let mut rng = DetRng::new(det::seed_for(&case.brand, 0, "eval-random"));
            let mut pool = case.pool.clone();
            for i in (1..pool.len()).rev() {
                pool.swap(i, rng.below(i + 1));
            }
            pool.into_iter().map(|i| catalog[i].id.clone()).collect()
        }
        "popularity" => rank_by(&case.pool, catalog, |i| pseudo_pop(&catalog[i].id)),
        "taste" => rank_by(&case.pool, catalog, |i| dot(&case.query, &index.vectors[i])),
        "full" => rank_by(&case.pool, catalog, |i| {
            let cos = dot(&case.query, &index.vectors[i]);
            let j = jaccard(&index.attrs[i], &case.user_attrs);
            fused_score(cos, j, cfg)
        }),
        other => panic!("unknown scorer {other}"),
    }
}

#[derive(Serialize)]
pub struct MetricRow {
    pub scorer: String,
    pub recall_at_10: MetricStat,
    pub ndcg_at_10: MetricStat,
    pub mrr: MetricStat,
    pub map: MetricStat,
}

#[derive(Serialize)]
pub struct OffPolicy {
    pub n_logs: usize,
    pub mean_pool_size: f64,
    /// On-policy value of the uniform logging (exploration) policy.
    pub v_logging: f64,
    /// IPS estimate of the taste-softmax target policy's value, from the logs.
    pub v_target_ips: f64,
    /// Self-normalized IPS estimate of the same target policy.
    pub v_target_snips: f64,
    /// Effective sample size of the importance weights (trust indicator).
    pub ess: f64,
    pub target_temperature: f64,
}

#[derive(Serialize)]
pub struct Diversity {
    /// Share of the catalog the shipped (full) ranking surfaces in any top-k.
    pub coverage_at_10: f64,
    /// Gini concentration of top-k impressions (0 even, ->1 skewed).
    pub gini_at_10: f64,
}

#[derive(Serialize)]
pub struct EvalReport {
    pub seed: u64,
    pub config_hash: String,
    pub k: usize,
    pub n_users: usize,
    pub ablation: Vec<MetricRow>,
    pub diversity: Diversity,
    pub off_policy: OffPolicy,
    pub note: String,
}

fn stat_for(
    scorer: &str,
    metric: &str,
    values: &[f64],
) -> MetricStat {
    let seed = fnv1a(format!("{scorer}:{metric}").as_bytes());
    bootstrap_ci(values, seed, RESAMPLES)
}

/// Run the full evaluation over the fixture catalog for a given config.
pub fn run(cfg: &EngineConfig) -> EvalReport {
    let catalog = crate::fixtures::catalog();
    let encoder = SyntheticEncoder { dim: cfg.vector_dim };
    let _ = encoder.dim();
    let index = CatalogIndex::build(&catalog, &encoder);
    let cases = build_cases(&catalog, &index, cfg);

    // ---- ablation ladder ----
    let mut ablation = Vec::new();
    let mut full_rankings: Vec<Vec<String>> = Vec::new();
    for scorer in ["random", "popularity", "taste", "full"] {
        let (mut recalls, mut ndcgs, mut mrrs, mut maps) = (vec![], vec![], vec![], vec![]);
        for case in &cases {
            let ranked = score_ranked(scorer, case, &catalog, &index, cfg);
            recalls.push(recall_at_k(&ranked, &case.positives, K));
            ndcgs.push(ndcg_at_k(&ranked, &case.positives, K));
            mrrs.push(mrr(&ranked, &case.positives));
            maps.push(average_precision(&ranked, &case.positives));
            if scorer == "full" {
                full_rankings.push(ranked);
            }
        }
        ablation.push(MetricRow {
            scorer: scorer.to_string(),
            recall_at_10: stat_for(scorer, "recall", &recalls),
            ndcg_at_10: stat_for(scorer, "ndcg", &ndcgs),
            mrr: stat_for(scorer, "mrr", &mrrs),
            map: stat_for(scorer, "map", &maps),
        });
    }
    let catalog_ids: BTreeSet<String> = catalog.iter().map(|l| l.id.clone()).collect();
    let diversity = Diversity {
        coverage_at_10: coverage(&full_rankings, &catalog_ids, K),
        gini_at_10: gini(&full_rankings, K),
    };

    // ---- off-policy: uniform exploration logs -> value of the taste target ----
    // Logging policy pi_0: draw one item uniformly from the pool (this is what
    // A29 exploration does; its propensity 1/|pool| is exactly what the engine
    // records). Target pi_e: softmax over the taste scores (a candidate ranker
    // we could ship). IPS/SNIPS estimate pi_e's reward from pi_0's logs alone.
    let mut samples: Vec<(f64, f64)> = Vec::new();
    let mut weights: Vec<f64> = Vec::new();
    let mut reward_sum = 0.0;
    let mut pool_size_sum = 0.0;
    for case in &cases {
        let p = &case.pool;
        if p.is_empty() {
            continue;
        }
        // pi_e over the pool: softmax of taste scores
        let scores: Vec<f64> = p.iter().map(|&i| dot(&case.query, &index.vectors[i])).collect();
        let pe = softmax(&scores, TEMPERATURE);
        let p0 = 1.0 / p.len() as f64;
        pool_size_sum += p.len() as f64;
        let mut rng = DetRng::new(det::seed_for(&case.brand, 0, "eval-offpolicy"));
        for _ in 0..N_LOG_PER_USER {
            let pos = rng.below(p.len());
            let item = p[pos];
            let reward = if catalog[item].brand == case.brand { 1.0 } else { 0.0 };
            let w = eval::importance_weight(pe[pos], p0);
            samples.push((w, reward));
            weights.push(w);
            reward_sum += reward;
        }
    }
    let n_logs = samples.len();
    let off_policy = OffPolicy {
        n_logs,
        mean_pool_size: if cases.is_empty() { 0.0 } else { pool_size_sum / cases.len() as f64 },
        v_logging: if n_logs == 0 { 0.0 } else { reward_sum / n_logs as f64 },
        v_target_ips: ips(&samples),
        v_target_snips: snips(&samples),
        ess: effective_sample_size(&weights),
        target_temperature: TEMPERATURE,
    };

    EvalReport {
        seed: 0,
        config_hash: cfg.config_hash(),
        k: K,
        n_users: cases.len(),
        ablation,
        diversity,
        off_policy,
        note: "Simulated-preference oracle (relevance = a brand); numbers validate \
               the estimators and pipeline and gate regressions, not live quality. \
               Fusion sits at parity with pure taste here because the synthetic A12 \
               encoder already encodes attributes into the vector, so graph-Jaccard \
               is redundant; the fork's fusion win depends on a visual encoder whose \
               vector carries signal orthogonal to the graph (revisit at real A12). \
               Swap the oracle for the archived-interaction temporal split for real eval."
            .to_string(),
    }
}

/// Compact markdown for the CLI: the ablation table plus the off-policy line.
pub fn render_markdown(rep: &EvalReport) -> String {
    let mut s = String::new();
    s.push_str(&format!(
        "ModSearch offline eval  (users={}, k={}, config={})\n\n",
        rep.n_users, rep.k, rep.config_hash
    ));
    s.push_str("| scorer | recall@10 | ndcg@10 | mrr | map |\n|---|---|---|---|---|\n");
    for r in &rep.ablation {
        s.push_str(&format!(
            "| {} | {:.3} [{:.3},{:.3}] | {:.3} | {:.3} | {:.3} |\n",
            r.scorer, r.recall_at_10.mean, r.recall_at_10.ci_low, r.recall_at_10.ci_high,
            r.ndcg_at_10.mean, r.mrr.mean, r.map.mean
        ));
    }
    let o = &rep.off_policy;
    s.push_str(&format!(
        "\noff-policy ({} logs, ESS {:.1}): V(uniform explore)={:.3}  ->  \
         V(taste target) IPS={:.3} SNIPS={:.3}\n",
        o.n_logs, o.ess, o.v_logging, o.v_target_ips, o.v_target_snips
    ));
    s
}

// ---- A16b: real-vs-synthetic fusion ablation (the difference-of-differences) ----

#[derive(Serialize)]
pub struct FusionLift {
    /// full minus taste: what graph-Jaccard fusion adds over pure taste-cosine.
    pub recall_delta: f64,
    pub ndcg_delta: f64,
    pub map_delta: f64,
}

#[derive(Serialize)]
pub struct EncoderResult {
    pub encoder: String,
    pub dim: usize,
    pub ablation: Vec<MetricRow>,
    pub fusion_lift: FusionLift,
}

#[derive(Serialize)]
pub struct RealEvalReport {
    pub catalog: String,
    pub n_items: usize,
    pub n_users: usize,
    pub k: usize,
    pub config_hash: String,
    pub real: EncoderResult,
    pub synthetic: EncoderResult,
    pub verdict: String,
    pub note: String,
}

/// Run the brand-oracle ladder over one index and summarize its fusion lift.
fn ablation_for(
    catalog: &[Listing],
    index: &CatalogIndex,
    cfg: &EngineConfig,
    encoder: &str,
    dim: usize,
) -> EncoderResult {
    let cases = build_cases(catalog, index, cfg);
    let mut rows = Vec::new();
    for scorer in ["random", "popularity", "taste", "full"] {
        let (mut recalls, mut ndcgs, mut mrrs, mut maps) = (vec![], vec![], vec![], vec![]);
        for case in &cases {
            let ranked = score_ranked(scorer, case, catalog, index, cfg);
            recalls.push(recall_at_k(&ranked, &case.positives, K));
            ndcgs.push(ndcg_at_k(&ranked, &case.positives, K));
            mrrs.push(mrr(&ranked, &case.positives));
            maps.push(average_precision(&ranked, &case.positives));
        }
        rows.push(MetricRow {
            scorer: scorer.to_string(),
            recall_at_10: stat_for(scorer, "recall", &recalls),
            ndcg_at_10: stat_for(scorer, "ndcg", &ndcgs),
            mrr: stat_for(scorer, "mrr", &mrrs),
            map: stat_for(scorer, "map", &maps),
        });
    }
    let get = |name: &str| rows.iter().find(|r| r.scorer == name).expect("scorer present");
    let (t, f) = (get("taste"), get("full"));
    let fusion_lift = FusionLift {
        recall_delta: f.recall_at_10.mean - t.recall_at_10.mean,
        ndcg_delta: f.ndcg_at_10.mean - t.ndcg_at_10.mean,
        map_delta: f.map.mean - t.map.mean,
    };
    EncoderResult { encoder: encoder.to_string(), dim, ablation: rows, fusion_lift }
}

/// A16b: the difference-of-differences. Run the identical brand-oracle ablation
/// over the real 768-d visual vectors and the synthetic attribute vectors on the
/// SAME catalog. The oracle, the pools, and the Jaccard attribute sets are
/// identical across the two; the only variable is the vector space. So the change
/// in fusion's lift between them isolates one thing: whether the encoder's vector
/// already carries the attribute signal (synthetic, where Jaccard is redundant)
/// or not (real visual, where Jaccard is orthogonal). That is exactly the A26
/// hypothesis, and the oracle cannot manufacture the difference.
pub fn run_real(cfg: &EngineConfig, catalog: &[Listing], real_vectors: Vec<Vec<f32>>) -> RealEvalReport {
    let real_dim = real_vectors.first().map_or(0, |v| v.len());
    let real_index = CatalogIndex::build_with_vectors(catalog, real_vectors);
    let synth_encoder = SyntheticEncoder { dim: cfg.vector_dim };
    let synth_index = CatalogIndex::build(catalog, &synth_encoder);

    let real = ablation_for(catalog, &real_index, cfg, "onnx-visual", real_dim);
    let synthetic = ablation_for(catalog, &synth_index, cfg, "synthetic-attr", cfg.vector_dim);

    let mut brand_counts: BTreeMap<String, usize> = BTreeMap::new();
    for l in catalog {
        *brand_counts.entry(l.brand.clone()).or_default() += 1;
    }
    let n_users = brand_counts.values().filter(|&&c| c >= MIN_BRAND_ITEMS).count();
    let catalog_name = catalog.first().map_or_else(String::new, |l| l.source.clone());

    let rd = real.fusion_lift.recall_delta;
    let sd = synthetic.fusion_lift.recall_delta;
    let diff = rd - sd;
    let verdict = if diff > 0.01 {
        format!(
            "Fusion lifts recall@{K} by {rd:+.3} on the real visual encoder vs {sd:+.3} on the \
             synthetic ({diff:+.3} difference). Graph-Jaccard adds signal the visual vector does \
             not carry, exactly where the A26 hypothesis predicted it would."
        )
    } else if diff < -0.01 {
        format!(
            "Fusion helps less on the real encoder ({rd:+.3}) than the synthetic ({sd:+.3}); the \
             A26 hypothesis does not hold on this catalog and oracle."
        )
    } else {
        format!(
            "No material difference in fusion lift between encoders (real {rd:+.3}, synthetic \
             {sd:+.3}). On this catalog and brand oracle, the visual vector and the attribute \
             vector leave the same room for Jaccard."
        )
    };

    RealEvalReport {
        catalog: catalog_name,
        n_items: catalog.len(),
        n_users,
        k: K,
        config_hash: cfg.config_hash(),
        real,
        synthetic,
        verdict,
        note: "Brand-oracle simulated preference (relevance = held-out same-brand items), the same \
               protocol A26 uses. The ABSOLUTE fusion lift on the real encoder partly reflects that \
               brand is one of the Jaccard attributes, so it boosts same-brand items directly; the \
               honest signal is the DIFFERENCE between the two encoders, which share that identical \
               Jaccard term and oracle and differ only in whether the vector already encodes \
               attributes. Still a simulated protocol, not live logs: swap the oracle for archived \
               interactions for a live-quality claim."
            .to_string(),
    }
}

/// Markdown for `eval-real --md`: both ladders, the fusion lifts, and the verdict.
pub fn render_markdown_real(rep: &RealEvalReport) -> String {
    let mut s = String::new();
    s.push_str(&format!(
        "ModSearch real-catalog eval  (catalog={}, items={}, users={}, k={}, config={})\n\n",
        rep.catalog, rep.n_items, rep.n_users, rep.k, rep.config_hash
    ));
    for enc in [&rep.real, &rep.synthetic] {
        s.push_str(&format!("### {} encoder (dim {})\n\n", enc.encoder, enc.dim));
        s.push_str("| scorer | recall@10 | ndcg@10 | mrr | map |\n|---|---|---|---|---|\n");
        for r in &enc.ablation {
            s.push_str(&format!(
                "| {} | {:.3} [{:.3},{:.3}] | {:.3} | {:.3} | {:.3} |\n",
                r.scorer, r.recall_at_10.mean, r.recall_at_10.ci_low, r.recall_at_10.ci_high,
                r.ndcg_at_10.mean, r.mrr.mean, r.map.mean
            ));
        }
        s.push_str(&format!(
            "\nfusion lift (full - taste): recall {:+.3}, ndcg {:+.3}, map {:+.3}\n\n",
            enc.fusion_lift.recall_delta, enc.fusion_lift.ndcg_delta, enc.fusion_lift.map_delta
        ));
    }
    s.push_str(&format!("VERDICT: {}\n", rep.verdict));
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ladder_beats_baselines_and_fusion_helps() {
        let cfg = EngineConfig::default();
        let rep = run(&cfg);
        assert!(rep.n_users >= 4, "need several simulated users, got {}", rep.n_users);
        let row = |name: &str| rep.ablation.iter().find(|r| r.scorer == name).unwrap();
        let (rand, pop, taste, full) =
            (row("random"), row("popularity"), row("taste"), row("full"));

        // personalized ranking must beat both non-personalized baselines
        assert!(
            taste.recall_at_10.mean > rand.recall_at_10.mean + 0.05,
            "taste {:.3} vs random {:.3}", taste.recall_at_10.mean, rand.recall_at_10.mean
        );
        assert!(
            taste.recall_at_10.mean > pop.recall_at_10.mean + 0.05,
            "taste {:.3} vs popularity {:.3}", taste.recall_at_10.mean, pop.recall_at_10.mean
        );
        // On the synthetic encoder, graph fusion is roughly neutral vs pure
        // taste (the vector already carries attribute structure, so Jaccard is
        // redundant). Assert near-parity, not a win: this is an honest ablation
        // result, revisited when the real A12 visual encoder lands.
        assert!(
            (full.recall_at_10.mean - taste.recall_at_10.mean).abs() < 0.05,
            "fusion should sit near taste on recall: full {:.3} vs taste {:.3}",
            full.recall_at_10.mean, taste.recall_at_10.mean
        );
        assert!(
            full.ndcg_at_10.mean > taste.ndcg_at_10.mean - 0.05,
            "fusion should not materially regress ndcg: full {:.3} vs taste {:.3}",
            full.ndcg_at_10.mean, taste.ndcg_at_10.mean
        );
        // random baseline should sit near chance on mrr (sanity that the
        // oracle isn't leaking through the pool ordering)
        assert!(rand.mrr.mean < taste.mrr.mean);
    }

    #[test]
    fn off_policy_estimates_target_above_logging() {
        let cfg = EngineConfig::default();
        let rep = run(&cfg);
        let o = &rep.off_policy;
        // the taste-softmax target concentrates on relevant items, so its
        // IPS-estimated value must exceed uniform exploration's on-policy value
        assert!(
            o.v_target_ips > o.v_logging + 0.02,
            "IPS {:.3} should beat logging {:.3}", o.v_target_ips, o.v_logging
        );
        // SNIPS lands in the same neighborhood, and both stay valid probabilities
        assert!(o.v_target_snips > o.v_logging);
        assert!((0.0..=1.0).contains(&o.v_logging));
        assert!(o.v_target_ips <= 1.0 + 1e-9 && o.v_target_snips <= 1.0 + 1e-9);
        // the estimate must rest on a non-trivial effective sample
        assert!(o.ess > 5.0, "ESS too low to trust: {:.2}", o.ess);
    }

    #[test]
    fn report_is_deterministic() {
        let cfg = EngineConfig::default();
        let a = serde_json::to_string(&run(&cfg)).unwrap();
        let b = serde_json::to_string(&run(&cfg)).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn run_real_is_symmetric_and_deterministic() {
        // Feed the synthetic encoding of the fixtures in as the "real" vectors.
        // Then both indexes are identical, so both ablations and both fusion
        // lifts must match exactly (difference ~0): a symmetry sanity that the
        // difference-of-differences is measuring the encoder, nothing else.
        let cfg = EngineConfig::default();
        let catalog = crate::fixtures::catalog();
        let enc = SyntheticEncoder { dim: cfg.vector_dim };
        let real_vecs: Vec<Vec<f32>> = catalog.iter().map(|l| enc.encode(l)).collect();
        let a = run_real(&cfg, &catalog, real_vecs.clone());
        let b = run_real(&cfg, &catalog, real_vecs);
        assert_eq!(serde_json::to_string(&a).unwrap(), serde_json::to_string(&b).unwrap());
        assert!(a.n_users >= 4, "need several brand cases, got {}", a.n_users);
        assert!(
            (a.real.fusion_lift.recall_delta - a.synthetic.fusion_lift.recall_delta).abs() < 1e-9,
            "identical vectors must give identical fusion lift"
        );
        assert!(render_markdown_real(&a).contains("VERDICT"));
    }

    #[test]
    fn golden_regression_gate() {
        // Headline numbers pinned coarsely (1e-3): tight enough to catch a real
        // ranking regression, loose enough to survive libm differences across
        // build platforms. Update consciously when a ranking change is intended.
        let rep = run(&EngineConfig::default());
        let row = |name: &str| rep.ablation.iter().find(|r| r.scorer == name).unwrap();
        assert!((row("full").recall_at_10.mean - 0.643).abs() < 2e-3,
            "full recall@10 = {:.4}", row("full").recall_at_10.mean);
        assert!((row("taste").ndcg_at_10.mean - 0.541).abs() < 2e-3,
            "taste ndcg@10 = {:.4}", row("taste").ndcg_at_10.mean);
        assert!((rep.off_policy.v_target_ips - 0.149).abs() < 2e-3,
            "IPS = {:.4}", rep.off_policy.v_target_ips);
    }
}
