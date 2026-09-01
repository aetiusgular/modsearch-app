//! Domain + wire model for the engine. Field names serialize to exactly the
//! shape the web app's `Listing` view model expects (ADR-0002: the SPA reads
//! engine JSON straight through the DataClient), so no mapping layer is needed
//! for the happy path. The contract's snake_case listing payload is mapped into
//! this view shape at ingest time (A16); here the stub emits it directly.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Serialize, Deserialize)]
pub struct PricePoint {
    pub t: String,
    pub price: f64,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Measurements {
    pub unit: String,
    pub values: BTreeMap<String, f64>,
    pub source_field: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Listing {
    pub id: String,
    pub brand: String,
    pub title: String,
    pub category: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subcategory: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub era: Option<String>,
    pub color: String,
    pub color_hex: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<String>,
    pub condition: String,
    pub price: f64,
    pub currency: String,
    pub source: String,
    pub source_kind: String,
    pub listing_url: String,
    pub measurements: Measurements,
    pub aesthetic: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
    pub listed_at: String,
    pub price_history: Vec<PricePoint>,
    pub match_score: f64,
    pub match_reasons: Vec<String>,
}

impl Listing {
    /// First recorded price vs current, as a percent (negative = drop).
    pub fn drop_pct(&self) -> i32 {
        let first = self.price_history.first().map(|p| p.price).unwrap_or(self.price);
        if first <= 0.0 {
            return 0;
        }
        (((self.price - first) / first) * 100.0).round() as i32
    }
    pub fn is_drop(&self) -> bool {
        self.drop_pct() <= -3
    }
}

/// Incoming query from the app. camelCase to match the SPA's FeedQuery.
#[derive(Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedQuery {
    pub text: Option<String>,
    pub measures: Option<BTreeMap<String, [f64; 2]>>,
    pub sizes: Option<Vec<String>>,
    pub colors: Option<Vec<String>>,
    pub brands: Option<Vec<String>>,
    pub price_min: Option<f64>,
    pub price_max: Option<f64>,
    pub conditions: Option<Vec<String>>,
    pub categories: Option<Vec<String>>,
    pub eras: Option<Vec<String>>,
    pub more_like_id: Option<String>,
    pub sort: Option<String>,
}
