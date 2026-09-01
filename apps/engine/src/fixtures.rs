//! Deterministic in-memory catalog for the A9 stub. Mirrors the shape of the
//! web app's fixtures so the engine and the mock render the same kind of data.
//! Replaced by the real local store (A10) + ingested catalog (A16) later.

use crate::model::{Listing, Measurements, PricePoint};
use std::collections::BTreeMap;

struct Rng(u64);
impl Rng {
    fn next_f(&mut self) -> f64 {
        // xorshift64*, mapped to [0,1)
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        ((x.wrapping_mul(0x2545F4914F6CDD1D) >> 11) as f64) / (1u64 << 53) as f64
    }
    fn pick<'a, T>(&mut self, a: &'a [T]) -> &'a T {
        &a[(self.next_f() * a.len() as f64) as usize]
    }
    fn between(&mut self, lo: f64, hi: f64) -> f64 {
        lo + self.next_f() * (hi - lo)
    }
}
fn round(n: f64, step: f64) -> f64 {
    (n / step).round() * step
}

const BRANDS: &[&str] = &[
    "Maison Margiela", "Rick Owens", "Kapital", "Stone Island", "Yohji Yamamoto",
    "Undercover", "Number (N)ine", "Raf Simons", "Comme des Garçons", "Issey Miyake",
    "Junya Watanabe", "visvim", "Needles", "Our Legacy", "Lemaire", "Auralee",
    "Stüssy", "Levi's", "Carhartt WIP", "Nike", "New Balance", "Salomon",
    "Arc'teryx", "Acronym", "Engineered Garments", "Bode", "Cav Empt", "Sacai",
];
const COLORS: &[(&str, &str)] = &[
    ("Black", "#1c1c1e"), ("Charcoal", "#3a3d42"), ("Slate", "#5b6570"),
    ("Ecru", "#d9d2c2"), ("Sand", "#c9b998"), ("Olive", "#5c5f3c"),
    ("Forest", "#2f4636"), ("Navy", "#26314a"), ("Indigo", "#3a4d74"),
    ("Rust", "#8a4a34"), ("Oxblood", "#5e2b2f"), ("Cream", "#eae3d2"),
    ("Grey Melange", "#8a8d90"), ("Brown", "#5a4433"), ("Burgundy", "#4e2230"),
    ("Washed Blue", "#7e97ac"),
];
const ERAS: &[&str] = &["1980s", "1990s", "Y2K", "2010s", "Contemporary"];
const AESTHETICS: &[&str] = &["Techwear", "Americana", "Avant-Garde", "Workwear", "Minimalism", "Gorpcore", "Archive", "Ivy"];
const SOURCES: &[(&str, &str)] = &[
    ("grailed", "marketplace"), ("ebay", "marketplace"), ("vestiaire", "marketplace"),
    ("depop", "marketplace"), ("shopify:no-man-walks-alone", "boutique"),
    ("shopify:lost-found", "boutique"), ("shopify:corlectic", "boutique"), ("agora", "marketplace"),
];
const CONDS: &[&str] = &["new", "like_new", "excellent", "good", "fair"];

// (category, subs, measure-keys with ranges, sizes, price range)
struct Cat {
    name: &'static str,
    subs: &'static [&'static str],
    measures: &'static [(&'static str, f64, f64)],
    sizes: &'static [&'static str],
    price: (f64, f64),
}
const CATS: &[Cat] = &[
    Cat { name: "Outerwear", subs: &["Field jacket", "Parka", "Trench", "Bomber", "Chore coat", "Leather jacket"],
        measures: &[("pit_to_pit", 54.0, 66.0), ("shoulder", 44.0, 54.0), ("length", 68.0, 86.0), ("sleeve", 62.0, 70.0)],
        sizes: &["44", "46", "48", "50", "S", "M", "L"], price: (180.0, 1400.0) },
    Cat { name: "Knitwear", subs: &["Crewneck", "Cardigan", "Mohair sweater", "Zip knit"],
        measures: &[("pit_to_pit", 50.0, 62.0), ("shoulder", 42.0, 52.0), ("length", 62.0, 76.0), ("sleeve", 60.0, 70.0)],
        sizes: &["S", "M", "L", "XL"], price: (90.0, 520.0) },
    Cat { name: "Tops", subs: &["Tee", "Overshirt", "Oxford shirt", "Rugby"],
        measures: &[("pit_to_pit", 48.0, 62.0), ("shoulder", 42.0, 52.0), ("length", 66.0, 78.0), ("sleeve", 20.0, 66.0)],
        sizes: &["S", "M", "L", "XL"], price: (40.0, 320.0) },
    Cat { name: "Bottoms", subs: &["Denim", "Trouser", "Cargo", "Wide pant"],
        measures: &[("waist", 72.0, 96.0), ("inseam", 66.0, 82.0), ("rise", 24.0, 34.0), ("thigh", 28.0, 40.0)],
        sizes: &["28", "30", "32", "34", "36"], price: (80.0, 620.0) },
    Cat { name: "Footwear", subs: &["Trainer", "Derby", "Boot", "GAT"],
        measures: &[], sizes: &["7", "8", "9", "10", "11", "12"], price: (90.0, 780.0) },
];

const DAY_MS: f64 = 86_400_000.0;

fn iso_days_ago(days: f64) -> String {
    // crude: fixed reference date, subtract days. Good enough for a stub sort key.
    let base = 20260901i64; // yyyymmdd anchor, only used for relative labels
    let _ = base;
    // produce a monotonic-ish ISO date by mapping day offset onto Sep 2026 window
    let d = (1 + (days as i64 % 28)).clamp(1, 28);
    format!("2026-08-{:02}", d)
}

pub fn catalog() -> Vec<Listing> {
    let mut rng = Rng(0x9E3779B97F4A7C15);
    (0..96)
        .map(|i| {
            let cat = rng.pick(CATS);
            let sub = *rng.pick(cat.subs);
            let (color, color_hex) = *rng.pick(COLORS);
            let (source, source_kind) = *rng.pick(SOURCES);
            let era = *rng.pick(ERAS);
            let base = round(rng.between(cat.price.0, cat.price.1), 5.0);
            let drops = rng.next_f() < 0.4;

            // price history
            let mut p = base * rng.between(1.05, 1.35);
            let n = 5 + (rng.next_f() * 4.0) as usize;
            let mut hist: Vec<PricePoint> = Vec::new();
            for k in (0..=n).rev() {
                if drops && rng.next_f() < 0.5 {
                    p = (base).max(p * rng.between(0.82, 0.97));
                }
                hist.push(PricePoint { t: iso_days_ago((k * 9) as f64), price: round(p, 1.0) });
            }
            if let Some(last) = hist.last_mut() {
                last.price = base;
            }

            // measurements
            let mut values: BTreeMap<String, f64> = BTreeMap::new();
            for (k, lo, hi) in cat.measures {
                values.insert((*k).to_string(), round(rng.between(*lo, *hi), 0.5));
            }
            let sf = {
                let r = rng.next_f();
                if r < 0.55 { "structured" } else if r < 0.7 { "parsed_text" } else if r < 0.85 { "ocr_photo" } else { "user_entered" }
            };

            let brand = *rng.pick(BRANDS);
            let cond = *rng.pick(CONDS);
            let match_score = (0.5 + (rng.next_f() - 0.4) * 0.9).clamp(0.32, 0.99);
            let color_owned = color.to_string();
            let reasons = vec![
                format!("{} you tend to like", color_owned),
                format!("{} match", rng.pick(AESTHETICS)),
            ];

            Listing {
                id: format!("it_{:03}", i + 1),
                brand: brand.to_string(),
                title: format!("{} {} {}", era, brand, sub),
                category: cat.name.to_string(),
                subcategory: Some(sub.to_string()),
                era: Some(era.to_string()),
                color: color.to_string(),
                color_hex: color_hex.to_string(),
                size: Some((*rng.pick(cat.sizes)).to_string()),
                condition: cond.to_string(),
                price: base,
                currency: "USD".to_string(),
                source: source.to_string(),
                source_kind: source_kind.to_string(),
                listing_url: format!("https://example.com/listing/{}", i + 1),
                measurements: Measurements { unit: "cm".to_string(), values, source_field: sf.to_string() },
                aesthetic: vec![(*rng.pick(AESTHETICS)).to_string()],
                image_url: None,
                listed_at: iso_days_ago(rng.between(0.0, 40.0)),
                price_history: hist,
                match_score,
                match_reasons: reasons,
            }
        })
        .collect()
}

// keep DAY_MS referenced to avoid dead-code warnings in the stub
#[allow(dead_code)]
const _DAY_REF: f64 = DAY_MS;
