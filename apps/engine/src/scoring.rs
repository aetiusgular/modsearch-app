//! A13 port, scoring half: dwell confidence, the coefficient matrix, episode
//! state, and R. Line-for-line port of the engine's `scoring/math.py`; the
//! golden table in `scoring/golden.py` is the parity contract and is asserted
//! here to 1e-5. No I/O, no clock.

use crate::config::EngineConfig;
use anyhow::{anyhow, Result};
use std::collections::BTreeSet;

/// Asymmetric dwell-time confidence weight (BUILD_PLANNING 1.1, hard_zero mode).
/// w(d) = 0 for d < min; dwell_coef * ln(1+d) on [min, max]; 0 beyond.
/// Corrupt dwell (negative, NaN, infinite) errors rather than scoring zero.
pub fn w_implicit(dwell_s: f64, cfg: &EngineConfig) -> Result<f64> {
    if !dwell_s.is_finite() || dwell_s < 0.0 {
        return Err(anyhow!("dwell_s must be finite and >= 0, got {dwell_s:?}"));
    }
    if dwell_s < cfg.dwell_min_s {
        return Ok(0.0);
    }
    if dwell_s <= cfg.dwell_max_s {
        return Ok(cfg.dwell_coef * dwell_s.ln_1p());
    }
    Ok(0.0)
}

/// C_event for one kind (1.2). Retractions negate their base only in extended
/// mode (D-04); otherwise they contribute 0.
pub fn coefficient_for(kind: &str, cfg: &EngineConfig) -> Result<f64> {
    let base = |k: &str| -> Option<f64> {
        match k {
            "save" => Some(cfg.coef_save),
            "comment" => Some(cfg.coef_comment),
            "inquiry" => Some(cfg.coef_inquiry),
            "like" => Some(cfg.coef_like),
            "click_detail" => Some(cfg.coef_click),
            _ => None,
        }
    };
    if let Some(c) = base(kind) {
        return Ok(c);
    }
    let retracts = match kind {
        "unlike" => Some("like"),
        "unsave" => Some("save"),
        _ => None,
    };
    if let Some(target) = retracts {
        return Ok(if cfg.extended_events { -base(target).unwrap() } else { 0.0 });
    }
    Err(anyhow!("event kind has no coefficient: {kind}"))
}

/// Kinds that carry an explicit coefficient (SCORED_KINDS + retractions).
fn scoreable(kind: &str) -> bool {
    matches!(kind, "save" | "comment" | "inquiry" | "like" | "click_detail" | "unlike" | "unsave")
}

/// Accumulated state of one (user, item) interaction episode (D-05).
/// Kinds count once (a set); dwell is the max over impression_end events;
/// event ids dedupe replays (at-least-once safety), capped at 64.
#[derive(Clone, Default)]
pub struct Episode {
    pub opened_ms: i64,
    pub last_ms: i64,
    pub dwell_ms: Option<i64>,
    pub kinds: BTreeSet<String>,
    pub event_ids: Vec<String>,
}

impl Episode {
    pub fn open(ts_ms: i64) -> Self {
        Self { opened_ms: ts_ms, last_ms: ts_ms, ..Default::default() }
    }

    /// Fold one event in. Exact duplicates (same event_id) are no-ops.
    pub fn apply(&mut self, event_id: &str, kind: &str, dwell_ms: Option<i64>, ts_ms: i64) {
        if self.event_ids.iter().any(|e| e == event_id) {
            return;
        }
        if kind == "impression_end" {
            let d = dwell_ms.unwrap_or(0);
            self.dwell_ms = Some(self.dwell_ms.map_or(d, |old| old.max(d)));
        }
        self.kinds.insert(kind.to_string());
        self.event_ids.push(event_id.to_string());
        if self.event_ids.len() > 64 {
            let drop = self.event_ids.len() - 64;
            self.event_ids.drain(0..drop);
        }
        self.last_ms = self.last_ms.max(ts_ms);
    }

    /// Reduce to (r_score, w_implicit) per 1.3: R = w(dwell) + sum of C_kind,
    /// each scored kind counted once, clamped at 0 (a retracted like is a
    /// non-event, not anti-taste; anti-taste is the explicit hide path).
    pub fn score(&self, cfg: &EngineConfig) -> Result<(f64, f64)> {
        let w = match self.dwell_ms {
            Some(ms) => w_implicit(ms as f64 / 1000.0, cfg)?,
            None => 0.0,
        };
        let mut explicit = 0.0;
        for kind in &self.kinds {
            if scoreable(kind) {
                explicit += coefficient_for(kind, cfg)?;
            }
        }
        Ok(((w + explicit).max(0.0), w))
    }

    pub fn kinds_csv(&self) -> String {
        self.kinds.iter().cloned().collect::<Vec<_>>().join(",")
    }
    pub fn ids_csv(&self) -> String {
        self.event_ids.join(",")
    }
    pub fn from_stored(
        opened_ms: i64,
        last_ms: i64,
        dwell_ms: Option<i64>,
        kinds_csv: &str,
        ids_csv: &str,
    ) -> Self {
        Self {
            opened_ms,
            last_ms,
            dwell_ms,
            kinds: kinds_csv.split(',').filter(|s| !s.is_empty()).map(String::from).collect(),
            event_ids: ids_csv.split(',').filter(|s| !s.is_empty()).map(String::from).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::EngineConfig;

    fn cfg() -> EngineConfig {
        EngineConfig::default()
    }

    #[test]
    fn golden_w_table_to_1e5() {
        // scoring/golden.py GOLDEN_W — the parity contract with the engine.
        let golden = [
            (1.0, 0.0),
            (1.5, 0.27489),
            (5.0, 0.53753),
            (12.0, 0.76949),
            (30.0, 1.03020),
            (45.0, 1.14859),
            (45.01, 0.0),
        ];
        for (d, want) in golden {
            let got = w_implicit(d, &cfg()).unwrap();
            assert!((got - want).abs() < 1e-5, "w({d}) = {got}, want {want}");
        }
        assert!(w_implicit(-1.0, &cfg()).is_err());
        assert!(w_implicit(f64::NAN, &cfg()).is_err());
    }

    #[test]
    fn golden_r_click_save_12s() {
        // {dwell 12 s, click_detail, save} -> 2.16949
        let mut ep = Episode::open(0);
        ep.apply("e1", "impression_end", Some(12_000), 100);
        ep.apply("e2", "click_detail", None, 200);
        ep.apply("e3", "save", None, 300);
        let (r, w) = ep.score(&cfg()).unwrap();
        assert!((r - 2.16949).abs() < 1e-5, "R = {r}");
        assert!((w - 0.76949).abs() < 1e-5);
    }

    #[test]
    fn golden_r_max() {
        // w(45) + save + comment + like + click_detail = 4.24859
        let mut ep = Episode::open(0);
        ep.apply("e1", "impression_end", Some(45_000), 1);
        ep.apply("e2", "save", None, 2);
        ep.apply("e3", "comment", None, 3);
        ep.apply("e4", "like", None, 4);
        ep.apply("e5", "click_detail", None, 5);
        let (r, _) = ep.score(&cfg()).unwrap();
        assert!((r - 4.24859).abs() < 1e-5, "R = {r}");
    }

    #[test]
    fn kinds_count_once_and_duplicates_are_noops() {
        let mut ep = Episode::open(0);
        ep.apply("e1", "like", None, 1);
        ep.apply("e1", "like", None, 1); // exact duplicate: dropped
        ep.apply("e2", "like", None, 2); // same kind again: set semantics
        let (r, _) = ep.score(&cfg()).unwrap();
        assert!((r - 0.8).abs() < 1e-9, "like must count once, got {r}");
    }

    #[test]
    fn dwell_takes_the_max_and_retraction_zeroes_in_base_mode() {
        let mut ep = Episode::open(0);
        ep.apply("e1", "impression_end", Some(5_000), 1);
        ep.apply("e2", "impression_end", Some(12_000), 2);
        ep.apply("e3", "impression_end", Some(3_000), 3);
        let (_, w) = ep.score(&cfg()).unwrap();
        assert!((w - 0.76949).abs() < 1e-5, "dwell must be max, w = {w}");

        // unlike contributes 0 with extended_events off, -0.8 with it on
        assert_eq!(coefficient_for("unlike", &cfg()).unwrap(), 0.0);
        let ext = EngineConfig { extended_events: true, ..EngineConfig::default() };
        assert!((coefficient_for("unlike", &ext).unwrap() + 0.8).abs() < 1e-9);
    }

    #[test]
    fn zero_score_episode_clamps_at_zero() {
        let mut ep = Episode::open(0);
        ep.apply("e1", "impression_end", Some(500), 1); // sub-threshold dwell
        let (r, w) = ep.score(&cfg()).unwrap();
        assert_eq!((r, w), (0.0, 0.0));
    }
}
