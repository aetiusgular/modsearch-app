//! A16: real-boutique ingest. Pull a Shopify storefront's tokenless
//! products.json, map each product to a `Listing`, embed its primary photo with
//! the CPU ONNX encoder (A12), and persist listing + real image vector.
//!
//! Gated behind the `ingest` feature: this is the only path that reaches the
//! network, and it only ever runs on a user's machine, never in CI. The encoder
//! is forced to CPU on purpose (the A12 determinism decision): the embeddings
//! are bit-identical wherever the same catalog is ingested, so the vector space
//! and the rankings on it are reproducible across machines.

use crate::embed::ImageEncoder;
use crate::embed_onnx::OnnxImageEncoder;
use crate::model::{Listing, Measurements, PricePoint};
use crate::store::Store;
use anyhow::{anyhow, Context, Result};
use serde_json::Value;
use std::collections::BTreeMap;
use std::io::Read;

const UA: &str = "ModSearch/0.1 (+https://github.com/aetiusgular/modsearch-app)";
const MAX_PAGES: usize = 40; // 40 * 250 = 10k items: a hard safety bound
const MAX_JSON_BYTES: u64 = 64 * 1024 * 1024;
const MAX_IMAGE_BYTES: u64 = 20 * 1024 * 1024;

pub struct Stats {
    pub embedded: usize,
    pub skipped: usize,
    pub dim: usize,
}

/// Ingest one Shopify storefront into `store`. Fetches every products.json page,
/// maps and embeds each product with a photo, and upserts listing + vector.
pub fn run(base_url: &str, store: &Store, model_path: &str) -> Result<Stats> {
    let base = base_url.trim_end_matches('/');
    let domain = domain_of(base);
    // CPU encoder: deterministic and cross-machine-identical (A12 decision).
    let enc = OnnxImageEncoder::new_forced(model_path, true)
        .context("building the CPU ONNX encoder")?;

    let mut listings: Vec<Listing> = Vec::new();
    let mut vectors: Vec<(String, Vec<f32>)> = Vec::new();
    let mut skipped = 0usize;

    for page in 1..=MAX_PAGES {
        let url = format!("{base}/products.json?limit=250&page={page}");
        let body = http_get_string(&url)
            .with_context(|| format!("fetching {url} (is products.json enabled on this store?)"))?;
        let json: Value = serde_json::from_str(&body)
            .with_context(|| format!("{url} did not return JSON; this may not be a Shopify storefront"))?;
        let products = match json["products"].as_array() {
            Some(a) if !a.is_empty() => a.clone(),
            _ => break, // empty page: end of catalog
        };
        for p in &products {
            let listing = match map_product(&domain, base, p) {
                Some(l) => l,
                None => {
                    skipped += 1;
                    continue;
                }
            };
            let img_url = listing.image_url.clone().expect("map_product requires an image");
            match http_get_bytes(&sized(&img_url)).and_then(|b| enc.encode_image(&b)) {
                Ok(v) => {
                    vectors.push((listing.id.clone(), v));
                    listings.push(listing);
                }
                Err(e) => {
                    eprintln!("[ingest] skip {}: {e}", listing.id);
                    skipped += 1;
                }
            }
        }
        eprintln!("[ingest] page {page}: {} embedded so far, {skipped} skipped", vectors.len());
    }

    store.seed_catalog(&listings).context("persisting listings")?;
    for (id, v) in &vectors {
        store.put_item_vector(id, v)?;
    }
    let dim = vectors.first().map(|(_, v)| v.len()).unwrap_or(0);
    Ok(Stats { embedded: vectors.len(), skipped, dim })
}

/// Map one Shopify product object to a `Listing`. None when it has no title or
/// no image (an item we cannot embed is not ingested, so the catalog stays a
/// clean set of image-backed vectors at one width).
fn map_product(domain: &str, base: &str, p: &Value) -> Option<Listing> {
    let pid = p["id"]
        .as_i64()
        .map(|n| n.to_string())
        .or_else(|| p["id"].as_str().map(str::to_string))?;
    let title = p["title"].as_str().unwrap_or("").trim().to_string();
    if title.is_empty() {
        return None;
    }
    let image_url = p["images"]
        .as_array()?
        .iter()
        .find_map(|im| im["src"].as_str())
        .map(str::to_string)?;

    let handle = p["handle"].as_str().unwrap_or("");
    let brand = p["vendor"].as_str().unwrap_or("").trim().to_string();
    let category = {
        let c = p["product_type"].as_str().unwrap_or("").trim();
        if c.is_empty() { "Apparel".to_string() } else { c.to_string() }
    };
    let tags = tags_of(p);
    let price = p["variants"]
        .as_array()
        .map(|vs| {
            vs.iter()
                .filter_map(|v| {
                    v["price"].as_str().and_then(|s| s.parse::<f64>().ok()).or_else(|| v["price"].as_f64())
                })
                .fold(f64::INFINITY, f64::min)
        })
        .filter(|p| p.is_finite())
        .unwrap_or(0.0);
    let (size, color_name) = size_and_color(p, &tags);
    let listed_at = p["published_at"].as_str().or_else(|| p["created_at"].as_str()).unwrap_or("").to_string();
    let day = listed_at.get(0..10).unwrap_or("").to_string();

    Some(Listing {
        id: format!("shopify:{domain}:{pid}"),
        brand,
        title,
        category,
        subcategory: None,
        era: None,
        color_hex: color_hex_for(&color_name),
        color: color_name,
        size,
        condition: "new".to_string(),
        price,
        currency: "USD".to_string(),
        source: format!("shopify:{domain}"),
        source_kind: "boutique".to_string(),
        listing_url: format!("{base}/products/{handle}"),
        measurements: Measurements {
            unit: "cm".to_string(),
            values: BTreeMap::new(),
            source_field: "none".to_string(),
        },
        aesthetic: tags.into_iter().take(6).collect(),
        image_url: Some(image_url),
        price_history: if day.len() == 10 { vec![PricePoint { t: day, price }] } else { Vec::new() },
        listed_at,
        match_score: 0.0,
        match_reasons: Vec::new(),
    })
}

fn tags_of(p: &Value) -> Vec<String> {
    match &p["tags"] {
        Value::Array(a) => a
            .iter()
            .filter_map(|t| t.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
        Value::String(s) => s.split(',').map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect(),
        _ => Vec::new(),
    }
}

/// Pull size + color from the product's option axes, falling back to a known
/// color word in the tags. Best-effort: these feed the Jaccard attribute set,
/// not correctness.
fn size_and_color(p: &Value, tags: &[String]) -> (Option<String>, String) {
    let mut size = None;
    let mut color = String::new();
    if let Some(opts) = p["options"].as_array() {
        for o in opts {
            let name = o["name"].as_str().unwrap_or("").to_ascii_lowercase();
            let first = o["values"].as_array().and_then(|vs| vs.first()).and_then(|x| x.as_str());
            if name.contains("size") {
                size = first.map(str::to_string);
            } else if name.contains("color") || name.contains("colour") {
                if let Some(c) = first {
                    color = c.trim().to_string();
                }
            }
        }
    }
    if color.is_empty() {
        if let Some(t) = tags.iter().find(|t| KNOWN_COLORS.iter().any(|(n, _)| t.eq_ignore_ascii_case(n))) {
            color = t.clone();
        }
    }
    (size, color)
}

const KNOWN_COLORS: &[(&str, &str)] = &[
    ("black", "#1c1c1e"), ("white", "#f2f2f0"), ("charcoal", "#3a3d42"), ("grey", "#8a8d90"),
    ("gray", "#8a8d90"), ("navy", "#26314a"), ("indigo", "#33406b"), ("blue", "#3a4d74"),
    ("forest", "#2f4636"), ("olive", "#5c5f3c"), ("green", "#3f5f43"), ("brown", "#5a4433"),
    ("tan", "#c9b998"), ("khaki", "#b3a06e"), ("beige", "#d9d2c2"), ("ecru", "#d9d2c2"),
    ("cream", "#eae3d2"), ("sand", "#c9b998"), ("stone", "#b9b2a4"), ("burgundy", "#4e2230"),
    ("oxblood", "#5e2b2f"), ("rust", "#8a4a34"), ("red", "#8a2f2f"), ("orange", "#c26a33"),
    ("yellow", "#c9a53a"), ("purple", "#4d3a63"), ("pink", "#c78fa0"), ("silver", "#c7ccce"),
];

fn color_hex_for(name: &str) -> String {
    let n = name.to_ascii_lowercase();
    for (key, hex) in KNOWN_COLORS {
        if n.contains(key) {
            return (*hex).to_string();
        }
    }
    "#808080".to_string()
}

fn domain_of(base: &str) -> String {
    let no_scheme = base.split_once("://").map(|(_, r)| r).unwrap_or(base);
    let host = no_scheme.split('/').next().unwrap_or(no_scheme);
    host.trim_start_matches("www.").to_string()
}

/// Ask the Shopify CDN for a 512px render instead of the full-size original; the
/// encoder resizes to 224 anyway, so this only saves bandwidth. Non-Shopify
/// hosts are left untouched.
fn sized(url: &str) -> String {
    if url.contains("cdn.shopify.com") || url.contains("/cdn/shop/") {
        let sep = if url.contains('?') { '&' } else { '?' };
        format!("{url}{sep}width=512")
    } else {
        url.to_string()
    }
}

fn http_get_string(url: &str) -> Result<String> {
    let resp = ureq::get(url).set("User-Agent", UA).call().map_err(|e| anyhow!("{e}"))?;
    let mut buf = Vec::new();
    resp.into_reader().take(MAX_JSON_BYTES).read_to_end(&mut buf)?;
    Ok(String::from_utf8(buf)?)
}

fn http_get_bytes(url: &str) -> Result<Vec<u8>> {
    let resp = ureq::get(url).set("User-Agent", UA).call().map_err(|e| anyhow!("{e}"))?;
    let mut buf = Vec::new();
    resp.into_reader().take(MAX_IMAGE_BYTES).read_to_end(&mut buf)?;
    if buf.is_empty() {
        return Err(anyhow!("empty image body"));
    }
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_a_shopify_product() {
        let p: Value = serde_json::from_str(
            r#"{
                "id": 123, "title": "Wool Chore Jacket", "handle": "wool-chore",
                "vendor": "Our Legacy", "product_type": "Outerwear",
                "tags": ["Workwear","AW24","Navy"],
                "published_at": "2026-08-20T00:00:00-04:00",
                "options": [{"name":"Size","values":["46","48","50"]},{"name":"Color","values":["Navy"]}],
                "variants": [{"price":"420.00"},{"price":"390.00"}],
                "images": [{"src":"https://cdn.shopify.com/s/files/img.jpg?v=1"}]
            }"#,
        )
        .unwrap();
        let l = map_product("shop.example", "https://shop.example", &p).unwrap();
        assert_eq!(l.id, "shopify:shop.example:123");
        assert_eq!(l.brand, "Our Legacy");
        assert_eq!(l.category, "Outerwear");
        assert_eq!(l.price, 390.0, "price is the min variant");
        assert_eq!(l.size.as_deref(), Some("46"));
        assert_eq!(l.color, "Navy");
        assert_eq!(l.source, "shopify:shop.example");
        assert_eq!(l.listing_url, "https://shop.example/products/wool-chore");
        assert!(l.aesthetic.contains(&"Workwear".to_string()));
        assert_eq!(l.image_url.as_deref(), Some("https://cdn.shopify.com/s/files/img.jpg?v=1"));
        assert_eq!(l.price_history.len(), 1);
    }

    #[test]
    fn color_falls_back_to_a_tag() {
        let p: Value = serde_json::from_str(
            r#"{"id":9,"title":"Tee","tags":["Cotton","Olive"],"images":[{"src":"x"}]}"#,
        )
        .unwrap();
        let l = map_product("d", "https://d", &p).unwrap();
        assert_eq!(l.color, "Olive");
        assert_eq!(l.color_hex, "#5c5f3c");
    }

    #[test]
    fn product_without_image_or_title_is_skipped() {
        let no_img: Value = serde_json::from_str(r#"{"id":1,"title":"No Photo","images":[]}"#).unwrap();
        assert!(map_product("d", "https://d", &no_img).is_none());
        let no_title: Value = serde_json::from_str(r#"{"id":2,"title":"","images":[{"src":"x"}]}"#).unwrap();
        assert!(map_product("d", "https://d", &no_title).is_none());
    }

    #[test]
    fn sized_appends_width_only_for_shopify_cdn() {
        assert_eq!(sized("https://cdn.shopify.com/x.jpg?v=1"), "https://cdn.shopify.com/x.jpg?v=1&width=512");
        assert_eq!(sized("https://cdn.shopify.com/x.jpg"), "https://cdn.shopify.com/x.jpg?width=512");
        assert_eq!(sized("https://other.com/x.jpg"), "https://other.com/x.jpg");
    }

    #[test]
    fn domain_strips_scheme_and_www() {
        assert_eq!(domain_of("https://www.shop.example/"), "shop.example");
        assert_eq!(domain_of("https://shop.example/path"), "shop.example");
    }
}
