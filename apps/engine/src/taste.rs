//! A13 port, profile half: the decay update, dual-window dual-space taste, and
//! the read-time blend. Ports `profiles/store.py`'s pure math exactly:
//! `compute_update_window` (1.4), `blend_query` (1.6, D-12), the
//! `max_norm_guard`, the e-folding tau (D-01), and the `dual_window` kill
//! switch. The Redis Lua transport becomes the SQLite Store; the atomicity
//! contract (all windows and spaces in one transaction) is preserved there.

use crate::config::EngineConfig;
use crate::store::Store;
use anyhow::{anyhow, Result};

pub const SHORT: &str = "short";
pub const LONG: &str = "long";
pub const SPACES: [&str; 2] = ["clip_base", "aesthetic"];

pub fn l2_norm(v: &[f32]) -> f64 {
    v.iter().map(|x| (*x as f64) * (*x as f64)).sum::<f64>().sqrt()
}

/// L2-normalized copy; zero-norm errors (ports codec.unit).
pub fn unit(v: &[f32]) -> Result<Vec<f32>> {
    let n = l2_norm(v);
    if n < 1e-9 {
        return Err(anyhow!("zero-norm vector cannot be normalized"));
    }
    Ok(v.iter().map(|x| (*x as f64 / n) as f32).collect())
}

/// The 1.4 update for ONE decay window with an explicit tau (ms).
/// First interaction: U = R*V. Else: U = lambda*U_old + (1-lambda)*R*V with
/// lambda = exp(-dt/tau), dt clamped at 0 (clock skew / replay). The norm
/// guard rescales defensively above `max_norm_guard`.
pub fn compute_update_window(
    u_old: Option<&[f32]>,
    last_update_ms: Option<i64>,
    r_score: f64,
    v: &[f32],
    now_ms: i64,
    cfg: &EngineConfig,
    tau_ms: f64,
) -> Result<Vec<f32>> {
    if !r_score.is_finite() || r_score < 0.0 {
        return Err(anyhow!("r_score must be finite and >= 0, got {r_score:?}"));
    }
    let mut out: Vec<f32> = match (u_old, last_update_ms) {
        (Some(u), Some(last)) => {
            let dt_ms = (now_ms - last).max(0) as f64;
            let lam = (-dt_ms / tau_ms).exp();
            let gain = 1.0 - lam; // spec mode (the engine's default update_mode)
            u.iter()
                .zip(v.iter())
                .map(|(uo, vi)| (lam * (*uo as f64) + gain * r_score * (*vi as f64)) as f32)
                .collect()
        }
        _ => v.iter().map(|vi| (r_score * (*vi as f64)) as f32).collect(),
    };
    let norm = l2_norm(&out);
    if norm > cfg.max_norm_guard {
        let scale = cfg.max_norm_guard / norm;
        for x in &mut out {
            *x = (*x as f64 * scale) as f32;
        }
    }
    Ok(out)
}

/// Blend two unit vectors into the retrieval query (1.6, D-12):
/// normalize(alpha*short + (1-alpha)*long); a degenerate (near-zero) blend
/// falls back to the short (recency) window.
pub fn blend_query(short_unit: &[f32], long_unit: &[f32], cfg: &EngineConfig) -> Vec<f32> {
    let a = cfg.blend_alpha;
    let blended: Vec<f32> = short_unit
        .iter()
        .zip(long_unit.iter())
        .map(|(s, l)| (a * (*s as f64) + (1.0 - a) * (*l as f64)) as f32)
        .collect();
    match unit(&blended) {
        Ok(u) => u,
        Err(_) => short_unit.to_vec(),
    }
}

/// Apply one scored interaction to every (space, window) atomically.
/// `vector` is the item embedding; at launch both spaces receive the same
/// vector (aesthetic mirrors clip_base until a trained encoder ships, D-07/D-08).
pub fn apply_interaction(
    store: &Store,
    user_key: &str,
    r_score: f64,
    vector: &[f32],
    now_ms: i64,
    cfg: &EngineConfig,
) -> Result<i64> {
    let meta = store.get_meta(user_key)?;
    let last_ms = meta.as_ref().map(|m| m.last_update_ms);
    let windows: [(&str, f64); 2] = [(SHORT, cfg.tau_ms()), (LONG, cfg.tau_long_ms())];
    let mut writes: Vec<(&str, &str, Vec<f32>)> = Vec::new();
    for space in SPACES {
        for (window, tau_ms) in windows {
            if !cfg.dual_window && window == LONG {
                continue;
            }
            let u_old = store.get_profile(user_key, space, window, cfg.vector_dim)?;
            let updated = compute_update_window(
                u_old.as_deref(),
                last_ms,
                r_score,
                vector,
                now_ms,
                cfg,
                tau_ms,
            )?;
            writes.push((space, window, updated));
        }
    }
    store.put_profiles(user_key, &writes, now_ms)
}

/// The retrieval query vector (1.6, D-12): None for an unknown user; the
/// short window's unit vector when single-window or the long twin is missing;
/// otherwise the alpha-blend of the two windows' unit vectors. The space is
/// `aesthetic` once the user clears `switch_to_aes_at` interactions, else
/// `clip_base` (identical values until a trained encoder ships).
pub fn query_vector(store: &Store, user_key: &str, cfg: &EngineConfig) -> Result<Option<Vec<f32>>> {
    let meta = match store.get_meta(user_key)? {
        None => return Ok(None),
        Some(m) => m,
    };
    let space = if meta.n_interactions >= cfg.switch_to_aes_at { "aesthetic" } else { "clip_base" };
    let short = match store.get_profile(user_key, space, SHORT, cfg.vector_dim)? {
        None => return Ok(None),
        Some(s) => s,
    };
    let short_unit = match unit(&short) {
        Ok(u) => u,
        Err(_) => return Ok(None), // zero-norm profile: treat as cold
    };
    if !cfg.dual_window {
        return Ok(Some(short_unit));
    }
    match store.get_profile(user_key, space, LONG, cfg.vector_dim)? {
        None => Ok(Some(short_unit)),
        Some(long) => match unit(&long) {
            Ok(long_unit) => Ok(Some(blend_query(&short_unit, &long_unit, cfg))),
            Err(_) => Ok(Some(short_unit)),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::EngineConfig;

    fn cfg() -> EngineConfig {
        EngineConfig { vector_dim: 4, ..EngineConfig::default() }
    }

    const DAY_MS: f64 = 86_400_000.0;

    #[test]
    fn golden_lambda_table_to_1e5() {
        // scoring/golden.py GOLDEN_LAMBDA at tau = 3 d.
        let golden = [
            (1.0 / 24.0, 0.98620),
            (1.0, 0.71653),
            (1.5, 0.60653),
            (3.0, 0.36788),
            (7.0, 0.09697),
        ];
        for (dt_days, want) in golden {
            let lam = (-(dt_days * DAY_MS) / (3.0 * DAY_MS)).exp();
            assert!((lam - want).abs() < 1e-5, "lambda({dt_days}d) = {lam}, want {want}");
        }
    }

    #[test]
    fn golden_update_r7() {
        // R=2.570, dt=36h => U_new = 0.60653*U_old + 1.01122*V (to 1e-4, the
        // engine's Lua/numpy parity bar).
        let c = cfg();
        let u_old = [1.0f32, 0.0, 0.0, 0.0];
        let v = [0.0f32, 1.0, 0.0, 0.0];
        let now = (36.0 * 3_600_000.0) as i64;
        let out = compute_update_window(Some(&u_old), Some(0), 2.570, &v, now, &c, c.tau_ms()).unwrap();
        assert!((out[0] as f64 - 0.60653).abs() < 1e-4, "lambda coeff = {}", out[0]);
        assert!((out[1] as f64 - 1.01122).abs() < 1e-4, "pull coeff = {}", out[1]);
    }

    #[test]
    fn first_interaction_is_rv_and_negative_dt_clamps() {
        let c = cfg();
        let v = [0.5f32, 0.5, 0.5, 0.5];
        let out = compute_update_window(None, None, 2.0, &v, 1000, &c, c.tau_ms()).unwrap();
        assert_eq!(out, vec![1.0f32, 1.0, 1.0, 1.0]);
        // clock skew: now < last => dt clamps to 0 => lambda 1, gain 0
        let same = compute_update_window(Some(&out), Some(5000), 3.0, &v, 4000, &c, c.tau_ms()).unwrap();
        assert_eq!(same, out);
        assert!(compute_update_window(None, None, -1.0, &v, 0, &c, c.tau_ms()).is_err());
    }

    #[test]
    fn norm_guard_rescales() {
        let c = EngineConfig { vector_dim: 2, max_norm_guard: 1.0, ..EngineConfig::default() };
        let v = [3.0f32, 4.0];
        let out = compute_update_window(None, None, 1.0, &v, 0, &c, c.tau_ms()).unwrap();
        assert!((l2_norm(&out) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn blend_falls_back_to_short_on_degenerate() {
        let c = cfg();
        let s = [1.0f32, 0.0, 0.0, 0.0];
        let l = [-1.0f32, 0.0, 0.0, 0.0];
        assert_eq!(blend_query(&s, &l, &c), s.to_vec()); // opposite: zero blend
        let l2 = [0.0f32, 1.0, 0.0, 0.0];
        let b = blend_query(&s, &l2, &c);
        assert!((l2_norm(&b) - 1.0).abs() < 1e-6);
        assert!((b[0] - b[1]).abs() < 1e-6); // alpha 0.5: symmetric
    }

    #[test]
    fn store_roundtrip_dual_window_dual_space() {
        let c = cfg();
        let store = Store::open(":memory:").unwrap();
        let v = unit(&[1.0f32, 1.0, 0.0, 0.0]).unwrap();
        let n = apply_interaction(&store, "u:t", 2.0, &v, 1_000, &c).unwrap();
        assert_eq!(n, 1);
        // all four (space, window) rows exist
        for space in SPACES {
            for w in [SHORT, LONG] {
                assert!(store.get_profile("u:t", space, w, 4).unwrap().is_some(), "{space}/{w}");
            }
        }
        let q = query_vector(&store, "u:t", &c).unwrap().unwrap();
        assert!((l2_norm(&q) - 1.0).abs() < 1e-6);
        assert!(query_vector(&store, "u:cold", &c).unwrap().is_none());
        // a second interaction on a different vector moves the query
        let v2 = unit(&[0.0f32, 0.0, 1.0, 1.0]).unwrap();
        apply_interaction(&store, "u:t", 2.0, &v2, 86_400_000 + 1_000, &c).unwrap();
        let q2 = query_vector(&store, "u:t", &c).unwrap().unwrap();
        assert!(q2[2] > q[2], "query must move toward the new item");
    }
}
