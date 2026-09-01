//! Query handlers over the catalog loaded from the store. A9 shipped these as
//! stubs; the A10/A13 port makes the default ordering real: once a user has
//! taste state, the feed ranks by the engine's fused score
//! (w_cos * [cos(U,V) * (1 + gamma * J)] + freshness + quality) through MMR with
//! brand caps, and "more like this" ranks by item-to-item fusion. Cold users
//! keep the fixture ordering (trending stand-in) so browsing never breaks.
//!
//! A28 rules: every ranked ordering goes through rank::rank_cmp / rank::asc_cmp
//! (total order, id tie-break), and randomness enters only through the seeded
//! DetRng handed in by dispatch. A31: the Reranker and ExplorationPolicy seams
//! are wired here so learned components land without touching this file.

use crate::config::EngineConfig;
use crate::det::DetRng;
use crate::model::{FeedQuery, Listing};
use crate::rank::{
    asc_cmp, explore_ratio_for, rank_cmp, ExplorationPolicy, ExploreRecord, RankCtx, Reranker,
};
use crate::retrieval::{base_score, dot, fused_score, jaccard, match_reasons, mmr_rank, Candidate, CatalogIndex};
use std::collections::{BTreeSet, HashSet};

const MEASURE_KEYS: &[&str] = &["pit_to_pit", "shoulder", "length", "sleeve", "waist", "hip", "inseam", "rise", "thigh"];

/// A user's live taste state at query time (None = cold start).
pub struct TasteState {
    pub query: Vec<f32>,
    pub attrs: BTreeSet<String>,
    pub n_interactions: i64,
}

/// Everything the ranked handlers need beyond the request itself.
pub struct FeedEnv<'a> {
    pub cfg: &'a EngineConfig,
    pub index: &'a CatalogIndex,
    pub taste: Option<TasteState>,
    pub day_epoch: i64,
}

fn contains_ci(hay: &str, needle: &str) -> bool {
    hay.to_lowercase().contains(&needle.to_lowercase())
}

fn matches(l: &Listing, q: &FeedQuery) -> bool {
    if let Some(t) = &q.text {
        if !t.is_empty() {
            let hay = format!(
                "{} {} {} {} {} {} {}",
                l.brand, l.title, l.category,
                l.subcategory.as_deref().unwrap_or(""), l.color,
                l.era.as_deref().unwrap_or(""), l.aesthetic.join(" ")
            );
            if !contains_ci(&hay, t) {
                return false;
            }
        }
    }
    let in_opt = |opt: &Option<Vec<String>>, v: &str| opt.as_ref().map_or(true, |xs| xs.iter().any(|x| x == v));
    if !in_opt(&q.brands, &l.brand) { return false; }
    if !in_opt(&q.colors, &l.color) { return false; }
    if !in_opt(&q.categories, &l.category) { return false; }
    if !in_opt(&q.conditions, &l.condition) { return false; }
    if let Some(sizes) = &q.sizes {
        match &l.size {
            Some(s) if sizes.iter().any(|x| x == s) => {}
            _ => return false,
        }
    }
    if let Some(eras) = &q.eras {
        match &l.era {
            Some(e) if eras.iter().any(|x| x == e) => {}
            _ => return false,
        }
    }
    if let Some(min) = q.price_min { if l.price < min { return false; } }
    if let Some(max) = q.price_max { if l.price > max { return false; } }
    if let Some(measures) = &q.measures {
        for k in MEASURE_KEYS {
            if let Some(r) = measures.get(*k) {
                match l.measurements.values.get(*k) {
                    Some(v) if *v >= r[0] && *v <= r[1] => {}
                    _ => return false, // lacking a filtered measure excludes the item
                }
            }
        }
    }
    true
}

/// True when the query asks for the default taste-ranked order, which is the
/// only ordering exploration may touch. An explicit user sort is respected
/// verbatim.
fn is_match_sort(q: &FeedQuery) -> bool {
    matches!(q.sort.as_deref(), None | Some("match")) && q.more_like_id.is_none()
}

/// Cold-start ordering: the fixture match_score stub (trending stand-in).
fn stub_sort(rows: &mut [Listing]) {
    rows.sort_by(|a, b| rank_cmp(a.match_score, &a.id, b.match_score, &b.id));
}

/// The ranked feed (getFeed and search). Filter, order (fused taste ranking
/// through MMR when taste exists, stub order when cold), apply the reranker
/// seam, then mix annealed exploration on the default ordering only.
pub fn feed(
    catalog: &[Listing],
    q: &FeedQuery,
    hidden: &HashSet<String>,
    env: &FeedEnv,
    rng: &mut DetRng,
    reranker: &dyn Reranker,
    explorer: &dyn ExplorationPolicy,
) -> (Vec<Listing>, Vec<ExploreRecord>) {
    let cfg = env.cfg;
    let mut rows: Vec<Listing> = match q.sort.as_deref() {
        Some("price_asc") => {
            let mut r = filtered(catalog, q, hidden);
            r.sort_by(|a, b| asc_cmp(a.price, &a.id, b.price, &b.id));
            r
        }
        Some("price_desc") => {
            let mut r = filtered(catalog, q, hidden);
            r.sort_by(|a, b| rank_cmp(a.price, &a.id, b.price, &b.id));
            r
        }
        Some("newest") => {
            let mut r = filtered(catalog, q, hidden);
            r.sort_by(|a, b| b.listed_at.cmp(&a.listed_at).then_with(|| a.id.cmp(&b.id)));
            r
        }
        _ => match &env.taste {
            Some(taste) => taste_ranked(catalog, q, hidden, taste, env),
            None => {
                let mut r = filtered(catalog, q, hidden);
                stub_sort(&mut r);
                r
            }
        },
    };

    reranker.rerank(&mut rows, &RankCtx { cfg });
    if is_match_sort(q) {
        let n = env.taste.as_ref().map_or(0, |t| t.n_interactions);
        let ratio = explore_ratio_for(n, cfg);
        explorer.mix(rows, ratio, cfg, rng)
    } else {
        (rows, Vec::new())
    }
}

fn filtered(catalog: &[Listing], q: &FeedQuery, hidden: &HashSet<String>) -> Vec<Listing> {
    catalog.iter().filter(|l| !hidden.contains(&l.id) && matches(l, q)).cloned().collect()
}

/// The real ranking path: fused relevance -> stable order -> MMR + brand cap,
/// with match_score/match_reasons rewritten to reflect the actual model.
fn taste_ranked(
    catalog: &[Listing],
    q: &FeedQuery,
    hidden: &HashSet<String>,
    taste: &TasteState,
    env: &FeedEnv,
) -> Vec<Listing> {
    let cfg = env.cfg;
    let idxs: Vec<usize> = catalog
        .iter()
        .enumerate()
        .filter(|(_, l)| !hidden.contains(&l.id) && matches(l, q))
        .map(|(i, _)| i)
        .collect();

    let mut cands: Vec<Candidate> = idxs
        .iter()
        .map(|&i| {
            let l = &catalog[i];
            let cos = dot(&taste.query, &env.index.vectors[i]);
            let j = jaccard(&env.index.attrs[i], &taste.attrs);
            let s = base_score(cos, j, env.index.listed_days[i], env.day_epoch, None, cfg);
            Candidate { idx: i, id: l.id.clone(), brand: l.brand.clone(), score: s }
        })
        .collect();
    cands.sort_by(|a, b| rank_cmp(a.score, &a.id, b.score, &b.id));

    let lo = cands.iter().map(|c| c.score).fold(f64::INFINITY, f64::min);
    let hi = cands.iter().map(|c| c.score).fold(f64::NEG_INFINITY, f64::max);
    let span = if (hi - lo).abs() < 1e-12 { 1.0 } else { hi - lo };
    let rel_of: std::collections::BTreeMap<usize, f64> =
        cands.iter().map(|c| (c.idx, (c.score - lo) / span)).collect();

    let order = mmr_rank(&cands, &env.index.vectors, cfg);
    order
        .into_iter()
        .map(|i| {
            let mut l = catalog[i].clone();
            let rel = rel_of.get(&i).copied().unwrap_or(0.0);
            l.match_score = (rel * 10_000.0).round() / 10_000.0;
            let reasons = match_reasons(&env.index.attrs[i], &taste.attrs);
            if !reasons.is_empty() {
                l.match_reasons = reasons;
            }
            l
        })
        .collect()
}

/// "More like this": item-to-item fused similarity (cos * (1 + gamma * J)),
/// top 8. Falls back to the stub ordering when the seed id is unknown.
pub fn more_like(
    catalog: &[Listing],
    id: &str,
    hidden: &HashSet<String>,
    env: &FeedEnv,
) -> Vec<Listing> {
    let cfg = env.cfg;
    let seed_idx = match env.index.by_id.get(id) {
        Some(&i) => i,
        None => {
            let mut rows: Vec<Listing> =
                catalog.iter().filter(|l| l.id != id && !hidden.contains(&l.id)).cloned().collect();
            stub_sort(&mut rows);
            rows.truncate(8);
            return rows;
        }
    };
    let mut scored: Vec<(usize, f64)> = catalog
        .iter()
        .enumerate()
        .filter(|(_, l)| l.id != id && !hidden.contains(&l.id))
        .map(|(i, _)| {
            let cos = dot(&env.index.vectors[seed_idx], &env.index.vectors[i]);
            let j = jaccard(&env.index.attrs[seed_idx], &env.index.attrs[i]);
            (i, fused_score(cos, j, cfg))
        })
        .collect();
    scored.sort_by(|a, b| rank_cmp(a.1, &catalog[a.0].id, b.1, &catalog[b.0].id));
    scored
        .into_iter()
        .take(8)
        .map(|(i, s)| {
            let mut l = catalog[i].clone();
            l.match_score = ((s.clamp(0.0, 1.25) / 1.25) * 10_000.0).round() / 10_000.0;
            let reasons = match_reasons(&env.index.attrs[i], &env.index.attrs[seed_idx]);
            if !reasons.is_empty() {
                l.match_reasons = reasons;
            }
            l
        })
        .collect()
}

pub fn saved(catalog: &[Listing], ids: &HashSet<String>) -> Vec<Listing> {
    catalog.iter().filter(|l| ids.contains(&l.id)).cloned().collect()
}

pub fn drops(catalog: &[Listing], hidden: &HashSet<String>) -> Vec<Listing> {
    let mut rows: Vec<Listing> = catalog.iter().filter(|l| !hidden.contains(&l.id) && l.is_drop()).cloned().collect();
    rows.sort_by(|a, b| a.drop_pct().cmp(&b.drop_pct()).then_with(|| a.id.cmp(&b.id)));
    rows
}

pub fn get_item<'a>(catalog: &'a [Listing], id: &str) -> Option<&'a Listing> {
    catalog.iter().find(|l| l.id == id)
}

/// Facet counts in the shape the app's FilterRail expects (FacetCounts).
pub fn facets(catalog: &[Listing]) -> serde_json::Value {
    use serde_json::json;
    use std::collections::BTreeMap;

    fn counted<'a>(it: impl Iterator<Item = &'a str>) -> Vec<(String, i64)> {
        let mut m: BTreeMap<String, i64> = BTreeMap::new();
        for v in it {
            if !v.is_empty() {
                *m.entry(v.to_string()).or_insert(0) += 1;
            }
        }
        let mut v: Vec<(String, i64)> = m.into_iter().collect();
        // sort_by is stable and the BTreeMap feeds it name-ascending, so equal
        // counts stay in name order deterministically.
        v.sort_by(|a, b| b.1.cmp(&a.1));
        v
    }

    let brands = counted(catalog.iter().map(|l| l.brand.as_str()));
    let categories = counted(catalog.iter().map(|l| l.category.as_str()));
    let conditions = counted(catalog.iter().map(|l| l.condition.as_str()));
    let eras = counted(catalog.iter().filter_map(|l| l.era.as_deref()));
    let sizes = counted(catalog.iter().filter_map(|l| l.size.as_deref()));

    // colors: name -> (hex, count)
    let mut cmap: BTreeMap<String, (String, i64)> = BTreeMap::new();
    for l in catalog {
        let e = cmap.entry(l.color.clone()).or_insert((l.color_hex.clone(), 0));
        e.1 += 1;
    }
    let mut colors: Vec<(String, String, i64)> = cmap.into_iter().map(|(n, (h, c))| (n, h, c)).collect();
    colors.sort_by(|a, b| b.2.cmp(&a.2));

    let prices: Vec<f64> = catalog.iter().map(|l| l.price).collect();
    let pmin = prices.iter().cloned().fold(f64::INFINITY, f64::min).floor();
    let pmax = prices.iter().cloned().fold(f64::NEG_INFINITY, f64::max).ceil();

    let keys = ["pit_to_pit", "shoulder", "length", "sleeve", "waist", "hip", "inseam", "rise", "thigh"];
    let mut measure_ranges = serde_json::Map::new();
    for k in keys {
        let vals: Vec<f64> = catalog.iter().filter_map(|l| l.measurements.values.get(k).cloned()).collect();
        if !vals.is_empty() {
            let lo = vals.iter().cloned().fold(f64::INFINITY, f64::min).floor();
            let hi = vals.iter().cloned().fold(f64::NEG_INFINITY, f64::max).ceil();
            measure_ranges.insert(k.to_string(), json!([lo, hi]));
        }
    }

    json!({
        "brands": brands,
        "colors": colors,
        "categories": categories,
        "conditions": conditions,
        "eras": eras,
        "sizes": sizes,
        "priceRange": [pmin, pmax],
        "measureRanges": measure_ranges,
    })
}
