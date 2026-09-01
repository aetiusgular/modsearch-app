//! A11-lite: entity attributes, Jaccard fusion, freshness-and-quality scoring,
//! and the MMR re-rank with brand caps. Ports `graph/entities.py`,
//! `graph/scorer.py`, and `feed/rerank.py`; the fusion golden
//! fused(0.85, 0.5) = 0.95625 > fused(0.90, 0) is asserted below.
//!
//! One deliberate divergence from the fork: candidates the MMR loop cannot
//! seat (brand-capped) are appended after the picked list in relevance order
//! instead of dropped. The fork assembles 40-item pages, where dropping is
//! correct; this client serves the full filtered list to the SPA, and a feed
//! that silently hides inventory would break filtered browsing.

use crate::config::EngineConfig;
use crate::model::Listing;
use std::collections::{BTreeMap, BTreeSet};

// ---- entities (graph/entities.py, adapted to the ModSearch listing shape) ----

/// `("brand", " Maison Margiela ") -> "brand:maison_margiela"`.
pub fn normalize_attr(namespace: &str, value: &str) -> Option<String> {
    let token = value.trim().to_lowercase().split_whitespace().collect::<Vec<_>>().join("_");
    if token.is_empty() {
        None
    } else {
        Some(format!("{namespace}:{token}"))
    }
}

/// Namespaced attribute tokens for one listing. The fork's namespaces are
/// (brand, era, designer, material); the ModSearch catalog carries brand and
/// era plus category/subcategory/color/condition/aesthetic, so those extend
/// the entity set (same normalization, same Jaccard semantics).
pub fn attribute_set(l: &Listing) -> BTreeSet<String> {
    let mut attrs = BTreeSet::new();
    let mut push = |ns: &str, v: &str| {
        if let Some(t) = normalize_attr(ns, v) {
            attrs.insert(t);
        }
    };
    push("brand", &l.brand);
    push("category", &l.category);
    push("color", &l.color);
    push("condition", &l.condition);
    if let Some(s) = &l.subcategory {
        push("subcategory", s);
    }
    if let Some(e) = &l.era {
        push("era", e);
    }
    for a in &l.aesthetic {
        push("aesthetic", a);
    }
    attrs
}

// ---- fusion (graph/scorer.py) ----

/// |A∩B| / |A∪B|; 0.0 when the union is empty.
pub fn jaccard(a: &BTreeSet<String>, b: &BTreeSet<String>) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 0.0;
    }
    let inter = a.intersection(b).count() as f64;
    let union = a.union(b).count() as f64;
    inter / union
}

/// fused = cos * (1 + gamma * J): entity affinity re-orders near-ties without
/// overruling the vector space.
pub fn fused_score(cosine: f64, jac: f64, cfg: &EngineConfig) -> f64 {
    cosine * (1.0 + cfg.gamma * jac)
}

// ---- dates (no chrono dep; civil-day arithmetic) ----

/// Days since the Unix epoch for a civil date (Howard Hinnant's algorithm).
pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u64;
    let mp = ((m + 9) % 12) as u64;
    let doy = (153 * mp + 2) / 5 + (d as u64 - 1);
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe as i64 - 719_468
}

/// Day epoch for a "YYYY-MM-DD..." string; None when unparseable.
pub fn listed_day(iso: &str) -> Option<i64> {
    let b = iso.as_bytes();
    if b.len() < 10 || b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    let y: i64 = iso.get(0..4)?.parse().ok()?;
    let m: u32 = iso.get(5..7)?.parse().ok()?;
    let d: u32 = iso.get(8..10)?.parse().ok()?;
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    Some(days_from_civil(y, m, d))
}

// ---- scoring + MMR (feed/rerank.py) ----

pub fn dot(a: &[f32], b: &[f32]) -> f64 {
    a.iter().zip(b.iter()).map(|(x, y)| *x as f64 * *y as f64).sum()
}

/// The scalar relevance before MMR (6.4, jaccard fusion mode):
/// w_cos*[cos*(1+gamma*J)] + w_fresh*exp(-age_d/tau_f) + w_quality*(rating/5, default 0.5).
pub fn base_score(
    cosine: f64,
    jac: f64,
    item_listed_day: Option<i64>,
    now_day: i64,
    seller_rating: Option<f64>,
    cfg: &EngineConfig,
) -> f64 {
    let age_days = item_listed_day.map_or(cfg.freshness_tau_days, |d| ((now_day - d).max(0)) as f64);
    let freshness = (-age_days / cfg.freshness_tau_days).exp();
    let quality = seller_rating.map_or(0.5, |r| r / 5.0);
    cfg.w_cos * fused_score(cosine, jac, cfg) + cfg.w_fresh * freshness + cfg.w_quality * quality
}

/// One scored candidate entering the re-rank.
pub struct Candidate {
    pub idx: usize, // index into the caller's row list
    pub id: String,
    pub brand: String,
    pub score: f64,
}

/// Deterministic MMR + rolling brand cap over the top `mmr_pool` candidates.
/// `vectors[cand.idx]` is that candidate's unit vector. Input must be sorted
/// by (score desc, id asc); ties inside the loop keep the earliest
/// (highest-relevance) candidate, so the whole pass is reproducible.
pub fn mmr_rank(cands: &[Candidate], vectors: &[Vec<f32>], cfg: &EngineConfig) -> Vec<usize> {
    let pool = cands.len().min(cfg.mmr_pool);
    if pool == 0 {
        return Vec::new();
    }
    let lo = cands[..pool].iter().map(|c| c.score).fold(f64::INFINITY, f64::min);
    let hi = cands[..pool].iter().map(|c| c.score).fold(f64::NEG_INFINITY, f64::max);
    let span = if (hi - lo).abs() < 1e-12 { 1.0 } else { hi - lo };

    let mut remaining: Vec<usize> = (0..pool).collect(); // indices into cands
    let mut picked: Vec<usize> = Vec::with_capacity(pool);
    let mut picked_brands: Vec<String> = Vec::new();
    let mut picked_idxs: Vec<usize> = Vec::new();

    while !remaining.is_empty() {
        let mut best: Option<(usize, f64)> = None; // (position in remaining, value)
        for (pos, &ci) in remaining.iter().enumerate() {
            let cand = &cands[ci];
            let mut diversity = 0.0f64;
            for &pi in &picked_idxs {
                let d = dot(&vectors[cand.idx], &vectors[pi]);
                if d > diversity {
                    diversity = d;
                }
            }
            let rel = (cand.score - lo) / span;
            let value = cfg.mmr_lambda * rel - (1.0 - cfg.mmr_lambda) * diversity;
            let allowed = brand_allowed(&cand.brand, &picked_brands, cfg);
            if allowed && best.map_or(true, |(_, bv)| value > bv) {
                best = Some((pos, value));
            }
        }
        match best {
            None => break, // every remaining candidate is brand-capped
            Some((pos, _)) => {
                let ci = remaining.remove(pos);
                picked_brands.push(cands[ci].brand.clone());
                picked_idxs.push(cands[ci].idx);
                picked.push(ci);
            }
        }
    }
    // Divergence from the fork (documented above): seat leftovers in relevance
    // order instead of dropping them, then everything past the MMR pool.
    let picked_set: BTreeSet<usize> = picked.iter().copied().collect();
    let mut out: Vec<usize> = picked.iter().map(|&ci| cands[ci].idx).collect();
    for ci in 0..pool {
        if !picked_set.contains(&ci) {
            out.push(cands[ci].idx);
        }
    }
    for c in &cands[pool..] {
        out.push(c.idx);
    }
    out
}

fn brand_allowed(brand: &str, picked_brands: &[String], cfg: &EngineConfig) -> bool {
    if brand.is_empty() {
        return true;
    }
    let start = picked_brands.len().saturating_sub(cfg.brand_window);
    picked_brands[start..].iter().filter(|b| b.as_str() == brand).count() < cfg.brand_cap
}

/// Human-readable "why" chips: the attributes the item shares with the user's
/// profile, prettified ("brand:rick_owens" -> "Rick Owens"), at most three.
pub fn match_reasons(item_attrs: &BTreeSet<String>, user_attrs: &BTreeSet<String>) -> Vec<String> {
    item_attrs
        .intersection(user_attrs)
        .take(3)
        .map(|t| {
            let raw = t.split_once(':').map_or(t.as_str(), |(_, v)| v);
            raw.split('_')
                .map(|w| {
                    let mut cs = w.chars();
                    match cs.next() {
                        Some(f) => f.to_uppercase().collect::<String>() + cs.as_str(),
                        None => String::new(),
                    }
                })
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect()
}

/// Precomputed retrieval state for the loaded catalog: unit vectors (through
/// the encoder seam), attribute sets, listed days, and an id index.
pub struct CatalogIndex {
    pub vectors: Vec<Vec<f32>>,
    pub attrs: Vec<BTreeSet<String>>,
    pub listed_days: Vec<Option<i64>>,
    pub by_id: BTreeMap<String, usize>,
}

impl CatalogIndex {
    pub fn build(catalog: &[Listing], encoder: &dyn crate::embed::ItemEncoder) -> Self {
        let vectors = catalog.iter().map(|l| encoder.encode(l)).collect();
        let attrs = catalog.iter().map(attribute_set).collect();
        let listed_days = catalog.iter().map(|l| listed_day(&l.listed_at)).collect();
        let by_id = catalog.iter().enumerate().map(|(i, l)| (l.id.clone(), i)).collect();
        Self { vectors, attrs, listed_days, by_id }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::EngineConfig;

    #[test]
    fn fused_golden() {
        // graph/scorer.py golden: fused(0.85, 0.5) = 0.95625 outranks fused(0.90, 0).
        let cfg = EngineConfig::default();
        let a = fused_score(0.85, 0.5, &cfg);
        let b = fused_score(0.90, 0.0, &cfg);
        assert!((a - 0.95625).abs() < 1e-9, "fused = {a}");
        assert!((b - 0.90).abs() < 1e-9);
        assert!(a > b);
    }

    #[test]
    fn jaccard_edges() {
        let s = |xs: &[&str]| xs.iter().map(|x| x.to_string()).collect::<BTreeSet<_>>();
        assert_eq!(jaccard(&s(&[]), &s(&[])), 0.0);
        assert_eq!(jaccard(&s(&["a", "b"]), &s(&["b", "c"])), 1.0 / 3.0);
        assert_eq!(jaccard(&s(&["a"]), &s(&["a"])), 1.0);
    }

    #[test]
    fn civil_days_and_listed_day() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(1970, 1, 2), 1);
        assert_eq!(days_from_civil(2026, 3, 1) - days_from_civil(2026, 2, 28), 1); // no leap day 2026
        assert_eq!(listed_day("2026-08-14"), Some(days_from_civil(2026, 8, 14)));
        assert_eq!(listed_day("garbage"), None);
    }

    #[test]
    fn normalize_and_reasons() {
        assert_eq!(normalize_attr("brand", " Maison Margiela "), Some("brand:maison_margiela".into()));
        assert_eq!(normalize_attr("era", "  "), None);
        let item: BTreeSet<String> =
            ["brand:rick_owens", "color:black"].iter().map(|s| s.to_string()).collect();
        let user: BTreeSet<String> =
            ["brand:rick_owens", "era:1990s"].iter().map(|s| s.to_string()).collect();
        assert_eq!(match_reasons(&item, &user), vec!["Rick Owens".to_string()]);
    }

    #[test]
    fn brand_cap_holds_in_window() {
        let cfg = EngineConfig::default();
        let picked: Vec<String> = vec!["x".into(), "x".into()];
        assert!(!brand_allowed("x", &picked, &cfg)); // cap 2 in window 20
        assert!(brand_allowed("y", &picked, &cfg));
        assert!(brand_allowed("", &picked, &cfg));
    }

    #[test]
    fn mmr_prefers_relevance_then_diversity_and_appends_leftovers() {
        let cfg = EngineConfig { mmr_pool: 3, ..EngineConfig::default() };
        let vecs: Vec<Vec<f32>> = vec![
            vec![1.0, 0.0], // 0: top relevance
            vec![1.0, 0.0], // 1: identical to 0 (redundant)
            vec![0.0, 1.0], // 2: orthogonal
            vec![0.5, 0.5], // 3: beyond the pool
        ];
        // scores chosen so the near-duplicate's relevance edge (rel 0.25 after
        // min-max) loses to its full diversity penalty at lambda 0.7
        let cands = vec![
            Candidate { idx: 0, id: "a".into(), brand: "b1".into(), score: 1.0 },
            Candidate { idx: 1, id: "b".into(), brand: "b2".into(), score: 0.985 },
            Candidate { idx: 2, id: "c".into(), brand: "b3".into(), score: 0.98 },
            Candidate { idx: 3, id: "d".into(), brand: "b4".into(), score: 0.10 },
        ];
        let order = mmr_rank(&cands, &vecs, &cfg);
        assert_eq!(order[0], 0, "highest relevance first");
        assert_eq!(order[1], 2, "orthogonal item must beat the near-duplicate");
        assert_eq!(order, vec![0, 2, 1, 3], "leftovers appended, pool tail last");
    }
}
