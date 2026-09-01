//! Query handlers over the in-memory catalog. Mirrors the web app's MockDataClient
//! logic so the mock and the engine behave identically; A11 replaces this with the
//! real ANN + graph-fusion retrieval ported from aura-recs-engine/ADAPTATION.md.

use crate::model::{FeedQuery, Listing};
use std::collections::HashSet;

const MEASURE_KEYS: &[&str] = &["pit_to_pit", "shoulder", "length", "sleeve", "waist", "hip", "inseam", "rise", "thigh"];

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

fn sort_rows(rows: &mut Vec<Listing>, q: &FeedQuery, catalog: &[Listing]) {
    match q.sort.as_deref() {
        Some("price_asc") => rows.sort_by(|a, b| a.price.partial_cmp(&b.price).unwrap()),
        Some("price_desc") => rows.sort_by(|a, b| b.price.partial_cmp(&a.price).unwrap()),
        Some("newest") => rows.sort_by(|a, b| b.listed_at.cmp(&a.listed_at)),
        _ => {
            let seed = q.more_like_id.as_ref().and_then(|id| catalog.iter().find(|x| &x.id == id));
            let score = |l: &Listing| -> f64 {
                let mut s = l.match_score;
                if let Some(sd) = seed {
                    if l.category == sd.category { s += 0.25; }
                    if l.color == sd.color { s += 0.12; }
                    if l.brand == sd.brand { s += 0.18; }
                }
                s
            };
            rows.sort_by(|a, b| score(b).partial_cmp(&score(a)).unwrap());
        }
    }
}

pub fn feed(catalog: &[Listing], q: &FeedQuery, hidden: &HashSet<String>) -> Vec<Listing> {
    let mut rows: Vec<Listing> = catalog.iter().filter(|l| !hidden.contains(&l.id) && matches(l, q)).cloned().collect();
    sort_rows(&mut rows, q, catalog);
    rows
}

pub fn more_like(catalog: &[Listing], id: &str, hidden: &HashSet<String>) -> Vec<Listing> {
    let q = FeedQuery { sort: Some("match".into()), more_like_id: Some(id.to_string()), ..Default::default() };
    let mut rows: Vec<Listing> = catalog.iter().filter(|l| l.id != id && !hidden.contains(&l.id)).cloned().collect();
    sort_rows(&mut rows, &q, catalog);
    rows.truncate(8);
    rows
}

pub fn saved(catalog: &[Listing], ids: &HashSet<String>) -> Vec<Listing> {
    catalog.iter().filter(|l| ids.contains(&l.id)).cloned().collect()
}

pub fn drops(catalog: &[Listing], hidden: &HashSet<String>) -> Vec<Listing> {
    let mut rows: Vec<Listing> = catalog.iter().filter(|l| !hidden.contains(&l.id) && l.is_drop()).cloned().collect();
    rows.sort_by_key(|l| l.drop_pct());
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
