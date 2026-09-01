//! EngineConfig (A28, extended by the A10/A13 port): every ranking constant
//! injectable, defaults pinned to the engine's `common/config.py` (the fork's
//! ScoringSettings, ProfileSettings, GraphSettings, FeedSettings). The
//! canonical serialization (struct field order) is hashed into `config_hash`,
//! stamped on every ranked response, so a feed is attributable to the exact
//! constants that produced it. Add fields at the end and expect the pinned
//! hash golden to move; that is the point.
//!
//! D-01 note carried over: tau_days is the e-folding time, not a half-life
//! (weight halves at tau*ln2, about 2.08 d for the default 3.0).

use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct EngineConfig {
    // ---- scoring (ScoringSettings) ----
    /// Dwell confidence (1.1): w = dwell_coef * ln(1 + d) on [min, max], else 0.
    pub dwell_min_s: f64,
    pub dwell_max_s: f64,
    pub dwell_coef: f64,
    /// Episode idle flush horizon (D-05), driven by event timestamps.
    pub episode_idle_flush_s: f64,
    /// D-04: retractions (unlike/unsave) negate their base only when on.
    pub extended_events: bool,
    /// Interaction coefficient matrix (1.2), BASE_COEFFICIENTS.
    pub coef_save: f64,
    pub coef_comment: f64,
    pub coef_inquiry: f64,
    pub coef_like: f64,
    pub coef_click: f64,

    // ---- profiles (ProfileSettings) ----
    pub tau_days: f64,
    pub tau_long_days: f64,
    pub blend_alpha: f64,
    pub dual_window: bool,
    pub vector_dim: usize,
    /// Feed queries switch from clip_base to the aesthetic space here.
    pub switch_to_aes_at: i64,
    pub max_norm_guard: f64,

    // ---- graph (GraphSettings) ----
    /// fused = cos * (1 + gamma * jaccard).
    pub gamma: f64,
    /// Last-K positively scored items feeding the attribute profile.
    pub history_k: usize,

    // ---- feed (FeedSettings) ----
    pub w_cos: f64,
    pub w_fresh: f64,
    pub w_quality: f64,
    pub freshness_tau_days: f64,
    pub mmr_lambda: f64,
    /// MMR runs over this many top candidates; the rest keep relevance order.
    pub mmr_pool: usize,
    pub brand_cap: usize,
    pub brand_window: usize,
    /// Exploration anneals from explore_max to explore_min over the user's
    /// first explore_anneal_n interactions (6.3).
    pub explore_max: f64,
    pub explore_min: f64,
    pub explore_anneal_n: i64,
    /// Ranked index where the exploration pool begins; items above it are
    /// never displaced by exploration.
    pub explore_pool_start: usize,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            dwell_min_s: 1.5,
            dwell_max_s: 45.0,
            dwell_coef: 0.3,
            episode_idle_flush_s: 60.0,
            extended_events: false,
            coef_save: 1.0,
            coef_comment: 0.9,
            coef_inquiry: 0.9,
            coef_like: 0.8,
            coef_click: 0.4,
            tau_days: 3.0,
            tau_long_days: 21.0,
            blend_alpha: 0.5,
            dual_window: true,
            vector_dim: 512,
            switch_to_aes_at: 10,
            max_norm_guard: 100.0,
            gamma: 0.25,
            history_k: 50,
            w_cos: 0.75,
            w_fresh: 0.15,
            w_quality: 0.10,
            freshness_tau_days: 14.0,
            mmr_lambda: 0.7,
            mmr_pool: 200,
            brand_cap: 2,
            brand_window: 20,
            explore_max: 0.30,
            explore_min: 0.10,
            explore_anneal_n: 50,
            explore_pool_start: 30,
        }
    }
}

impl EngineConfig {
    pub fn tau_ms(&self) -> f64 {
        self.tau_days * 86_400_000.0
    }
    pub fn tau_long_ms(&self) -> f64 {
        self.tau_long_days * 86_400_000.0
    }
    pub fn episode_idle_flush_ms(&self) -> i64 {
        (self.episode_idle_flush_s * 1000.0) as i64
    }

    /// Load from the JSON file at $AURA_ENGINE_CONFIG, else defaults.
    /// A bad file is a hard error: a silently ignored config would serve
    /// rankings nobody can attribute.
    pub fn load() -> anyhow::Result<Self> {
        match std::env::var("AURA_ENGINE_CONFIG") {
            Ok(path) => {
                let text = std::fs::read_to_string(&path)?;
                Ok(serde_json::from_str(&text)?)
            }
            Err(_) => Ok(Self::default()),
        }
    }

    /// Stable hash of the canonical serialization.
    pub fn config_hash(&self) -> String {
        let bytes = serde_json::to_vec(self).expect("config serializes");
        format!("{:016x}", crate::det::fnv1a(&bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_hash_is_pinned() {
        // Golden. If a ranking default changes, this moves, and it must move
        // consciously: update the pin in the same commit that changes the
        // default, and say why in the commit message.
        assert_eq!(EngineConfig::default().config_hash(), "efbbd295115d3837");
    }

    #[test]
    fn config_hash_tracks_values() {
        let mut c = EngineConfig::default();
        let base = c.config_hash();
        c.gamma = 0.30;
        assert_ne!(base, c.config_hash());
    }
}
