//! ModSearch headless engine service.
//! A9: framed native-messaging loop + oneshot mode. A28-A31: determinism
//! kernel (seeds, provenance, goldens, seams). A10: SQLite persistence.
//! A13 port: real scoring, episodes, and dual-window dual-space taste.
//! A11-lite: fused retrieval (cos * (1 + gamma * J) + freshness + quality)
//! through MMR with brand caps, over deterministic synthetic embeddings
//! behind the A12 encoder seam.
//!
//! Determinism contract: every ranked response is a pure function of
//! (store state, config, userId, dayEpoch); every state change is a pure
//! function of the request stream (event client_ts / request ts). The only
//! wall-clock reads are the dayEpoch and ts defaults below; pass both
//! explicitly to replay exactly.

mod config;
mod det;
mod embed;
#[cfg(feature = "onnx")]
mod embed_onnx;
mod eval;
mod evalrun;
mod fixtures;
mod handlers;
mod model;
mod protocol;
mod rank;
mod retrieval;
mod scoring;
mod store;
mod taste;

use embed::SyntheticEncoder;
use handlers::{FeedEnv, TasteState};
use model::Listing;
use protocol::RawRequest;
use rank::{IdentityReranker, SeededFraction};
use retrieval::CatalogIndex;
use serde_json::json;
use std::collections::HashSet;
use std::io::Read;
use store::Store;

/// Immutable per-process context: the config, its pinned hash, and a stable
/// fingerprint of the loaded catalog.
struct Ctx {
    cfg: config::EngineConfig,
    cfg_hash: String,
    catalog_fp: String,
}

impl Ctx {
    fn new(cfg: config::EngineConfig, catalog: &[Listing]) -> Self {
        let cfg_hash = cfg.config_hash();
        let catalog_fp = catalog_fingerprint(catalog);
        Self { cfg, cfg_hash, catalog_fp }
    }
}

/// Stable fingerprint of the catalog contents (ids + prices).
fn catalog_fingerprint(catalog: &[Listing]) -> String {
    let mut bytes = Vec::new();
    for l in catalog {
        bytes.extend_from_slice(l.id.as_bytes());
        bytes.push(0x1f);
        bytes.extend_from_slice(&l.price.to_le_bytes());
        bytes.push(0x1e);
    }
    format!("{:016x}", det::fnv1a(&bytes))
}

/// Milliseconds since the Unix epoch. One of the two wall-clock reads in the
/// engine; only the defaults below use it.
fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn today_epoch() -> u64 {
    (now_ms() / 86_400_000) as u64
}

/// Parse an ISO-8601 timestamp ("2026-09-01T12:30:05.250Z", "+02:00" offsets)
/// to ms since epoch. None when unparseable.
fn parse_client_ts(iso: &str) -> Option<i64> {
    let day = retrieval::listed_day(iso.get(0..10)?)?;
    let rest = iso.get(10..)?;
    if !rest.starts_with('T') {
        return None;
    }
    let time = rest.get(1..)?;
    let h: i64 = time.get(0..2)?.parse().ok()?;
    let m: i64 = time.get(3..5)?.parse().ok()?;
    let s: i64 = time.get(6..8)?.parse().ok()?;
    let mut idx = 8;
    let bytes = time.as_bytes();
    let mut frac_ms: i64 = 0;
    if bytes.get(idx) == Some(&b'.') {
        let start = idx + 1;
        let mut end = start;
        while end < bytes.len() && bytes[end].is_ascii_digit() {
            end += 1;
        }
        let digits = time.get(start..end)?;
        let scaled: String = format!("{digits}000").chars().take(3).collect();
        frac_ms = scaled.parse().ok()?;
        idx = end;
    }
    let offset_min: i64 = match bytes.get(idx) {
        Some(&b'Z') => 0,
        Some(&b'+') | Some(&b'-') => {
            let sign = if bytes[idx] == b'+' { 1 } else { -1 };
            let oh: i64 = time.get(idx + 1..idx + 3)?.parse().ok()?;
            let om: i64 = time.get(idx + 4..idx + 6)?.parse().ok()?;
            sign * (oh * 60 + om)
        }
        _ => 0, // naive timestamps are treated as UTC
    };
    Some(day * 86_400_000 + ((h * 60 + m) * 60 + s) * 1000 + frac_ms - offset_min * 60_000)
}

fn user_key(user_id: &str) -> String {
    format!("u:{user_id}")
}

/// Assemble the user's live taste state (query vector, attribute profile,
/// interaction count); None for a cold user.
fn build_taste(
    st: &Store,
    ukey: &str,
    cfg: &config::EngineConfig,
    index: &CatalogIndex,
) -> anyhow::Result<Option<TasteState>> {
    let query = match taste::query_vector(st, ukey, cfg)? {
        None => return Ok(None),
        Some(q) => q,
    };
    let n = st.get_meta(ukey)?.map_or(0, |m| m.n_interactions);
    let mut attrs = std::collections::BTreeSet::new();
    for item in st.recent_history(ukey, cfg.history_k)? {
        if let Some(&i) = index.by_id.get(&item) {
            attrs.extend(index.attrs[i].iter().cloned());
        }
    }
    Ok(Some(TasteState { query, attrs, n_interactions: n }))
}

/// Score and apply one closed episode: taste update + graph history +
/// interaction log. Returns true when the episode moved the profile.
fn flush_episode(
    st: &Store,
    index: &CatalogIndex,
    cfg: &config::EngineConfig,
    ukey: &str,
    item_id: &str,
    ep: &scoring::Episode,
) -> anyhow::Result<bool> {
    st.delete_episode(ukey, item_id)?;
    let (r, w) = ep.score(cfg)?;
    if r <= 0.0 {
        return Ok(false);
    }
    st.record_interaction(ukey, item_id, r, w, ep.last_ms)?;
    st.record_history(ukey, item_id, ep.last_ms)?;
    if let Some(&i) = index.by_id.get(item_id) {
        taste::apply_interaction(st, ukey, r, &index.vectors[i], ep.last_ms, cfg)?;
    }
    Ok(true)
}

const EPISODE_KINDS: &[&str] = &[
    "impression_start", "impression_end", "click_detail", "like", "unlike",
    "save", "unsave", "comment", "inquiry",
];

fn dispatch(
    catalog: &[Listing],
    index: &CatalogIndex,
    st: &Store,
    ctx: &Ctx,
    raw: &RawRequest,
) -> serde_json::Value {
    let hidden: HashSet<String> = raw.hidden.iter().cloned().collect();

    let user_id = raw.user_id.as_deref().unwrap_or("local");
    let ukey = user_key(user_id);
    let day_epoch = raw.day_epoch.unwrap_or_else(today_epoch);
    let ts = raw.ts.unwrap_or_else(now_ms);

    let mut provenance: Option<serde_json::Value> = None;

    let data: Result<serde_json::Value, String> = (|| -> anyhow::Result<serde_json::Value> {
        Ok(match raw.kind.as_str() {
            "ping" => json!({ "pong": true, "version": env!("CARGO_PKG_VERSION") }),
            "getFeed" | "search" => {
                let taste_state = build_taste(st, &ukey, &ctx.cfg, index)?;
                let n_interactions = taste_state.as_ref().map_or(0, |t| t.n_interactions);
                let env = FeedEnv { cfg: &ctx.cfg, index, taste: taste_state, day_epoch: day_epoch as i64 };
                let seed = det::seed_for(user_id, day_epoch, "feed-explore");
                let mut rng = det::DetRng::new(seed);
                let (rows, explore) = handlers::feed(
                    catalog, &raw.query, &hidden, &env, &mut rng, &IdentityReranker, &SeededFraction,
                );
                provenance = Some(json!({
                    "engineVersion": env!("CARGO_PKG_VERSION"),
                    "configHash": ctx.cfg_hash,
                    "catalog": ctx.catalog_fp,
                    "userId": user_id,
                    "dayEpoch": day_epoch,
                    "seed": format!("{:016x}", seed),
                    "tasteSnapshot": st.taste_fingerprint(&ukey)?.unwrap_or_else(|| "cold".into()),
                    "nInteractions": n_interactions,
                    "explore": explore,
                }));
                json!(rows)
            }
            "getItem" => match raw.item_id.as_deref() {
                Some(id) => json!(handlers::get_item(catalog, id)),
                None => anyhow::bail!("getItem requires itemId"),
            },
            "moreLikeThis" => match raw.item_id.as_deref() {
                Some(id) => {
                    let env = FeedEnv { cfg: &ctx.cfg, index, taste: None, day_epoch: day_epoch as i64 };
                    json!(handlers::more_like(catalog, id, &hidden, &env))
                }
                None => anyhow::bail!("moreLikeThis requires itemId"),
            },
            "getFacets" => handlers::facets(catalog),
            "getSaved" => {
                let mut ids: HashSet<String> = raw.saved_ids.iter().cloned().collect();
                ids.extend(st.saved_ids(&ukey)?);
                json!(handlers::saved(catalog, &ids))
            }
            "getDrops" => json!(handlers::drops(catalog, &hidden)),
            "recordFeedback" => {
                let item_id = raw.item_id.as_deref()
                    .ok_or_else(|| anyhow::anyhow!("recordFeedback requires itemId"))?;
                let kind = raw.feedback_kind.as_deref()
                    .ok_or_else(|| anyhow::anyhow!("recordFeedback requires feedbackKind"))?;
                st.record_feedback(&ukey, item_id, kind, ts)?;
                let mut n = st.get_meta(&ukey)?.map_or(0, |m| m.n_interactions);
                let mut applied = false;
                match kind {
                    // D-04 semantics, matching the fork: hide is not an
                    // anti-taste vector update (compute_update rejects negative
                    // R, and episode scores clamp at 0). Hiding voids any open
                    // episode so pending positives never score, and the client
                    // keeps the item out via the hidden list.
                    "hide" => {
                        st.delete_episode(&ukey, item_id)?;
                    }
                    "like" | "save" => {
                        if let Some(&i) = index.by_id.get(item_id) {
                            let r = if kind == "like" { ctx.cfg.coef_like } else { ctx.cfg.coef_save };
                            n = taste::apply_interaction(st, &ukey, r, &index.vectors[i], ts, &ctx.cfg)?;
                            st.record_history(&ukey, item_id, ts)?;
                            st.record_interaction(&ukey, item_id, r, 0.0, ts)?;
                            applied = true;
                        }
                    }
                    other => anyhow::bail!("unknown feedbackKind: {other}"),
                }
                json!({ "recorded": true, "itemId": item_id, "kind": kind, "applied": applied, "nInteractions": n })
            }
            "recordEvents" => {
                let idle_ms = ctx.cfg.episode_idle_flush_ms();
                let mut recorded = 0usize;
                let mut flushed = 0usize;
                let mut skipped = 0usize;
                let mut max_ts = ts;
                for ev in &raw.events {
                    let (Some(kind), Some(event_id)) = (ev["type"].as_str(), ev["event_id"].as_str()) else {
                        skipped += 1;
                        continue;
                    };
                    let ev_user = ev["user_id"].as_str().unwrap_or(user_id);
                    let ev_ukey = user_key(ev_user);
                    let ev_ts = ev["client_ts"].as_str().and_then(parse_client_ts).unwrap_or(ts);
                    max_ts = max_ts.max(ev_ts);
                    let item = ev["item_id"].as_str();
                    if !st.insert_event(event_id, &ev_ukey, item, kind, ev_ts, &ev.to_string())? {
                        continue; // duplicate replay: at-least-once safety
                    }
                    recorded += 1;
                    let (Some(item_id), true) = (item, EPISODE_KINDS.contains(&kind)) else {
                        continue;
                    };
                    let mut ep = match st.get_episode(&ev_ukey, item_id)? {
                        Some((opened, last, dwell, kinds, ids)) => {
                            let existing = scoring::Episode::from_stored(opened, last, dwell, &kinds, &ids);
                            if ev_ts - existing.last_ms > idle_ms {
                                if flush_episode(st, index, &ctx.cfg, &ev_ukey, item_id, &existing)? {
                                    flushed += 1;
                                }
                                scoring::Episode::open(ev_ts)
                            } else {
                                existing
                            }
                        }
                        None => scoring::Episode::open(ev_ts),
                    };
                    ep.apply(event_id, kind, ev["dwell_ms"].as_i64(), ev_ts);
                    if kind == "impression_end" {
                        if flush_episode(st, index, &ctx.cfg, &ev_ukey, item_id, &ep)? {
                            flushed += 1;
                        }
                    } else {
                        st.put_episode(
                            &ev_ukey, item_id, ep.opened_ms, ep.last_ms, ep.dwell_ms,
                            &ep.kinds_csv(), &ep.ids_csv(),
                        )?;
                    }
                }
                for (uk, item) in st.idle_episodes(max_ts, idle_ms)? {
                    if let Some((opened, last, dwell, kinds, ids)) = st.get_episode(&uk, &item)? {
                        let ep = scoring::Episode::from_stored(opened, last, dwell, &kinds, &ids);
                        if flush_episode(st, index, &ctx.cfg, &uk, &item, &ep)? {
                            flushed += 1;
                        }
                    }
                }
                json!({ "recorded": recorded, "flushed": flushed, "skipped": skipped })
            }
            other => anyhow::bail!("unknown request type: {other}"),
        })
    })()
    .map_err(|e| e.to_string());

    match data {
        Ok(d) => {
            let mut env = json!({ "id": raw.id, "ok": true, "type": raw.kind, "data": d });
            if let Some(p) = provenance {
                env["provenance"] = p;
            }
            env
        }
        Err(e) => json!({ "id": raw.id, "ok": false, "type": raw.kind, "error": e }),
    }
}

fn db_path() -> String {
    if let Ok(p) = std::env::var("MOD_ENGINE_DB") {
        return p;
    }
    let home = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE"));
    match home {
        Ok(h) => format!("{h}/.modsearch/engine.db"),
        Err(_) => "modsearch-engine.db".to_string(),
    }
}

fn main() -> anyhow::Result<()> {
    // A12 (onnx feature): `embed-image <path>` loads the vision model from
    // MOD_ENGINE_MODEL, logs the selected provider, and prints the L2-normalized
    // embedding. The on-device way to sanity-check CoreML/DirectML + the model.
    #[cfg(feature = "onnx")]
    if std::env::args().nth(1).as_deref() == Some("embed-image") {
        use embed::ImageEncoder;
        let path = std::env::args().nth(2)
            .ok_or_else(|| anyhow::anyhow!("usage: modsearch-engine embed-image <image-path>"))?;
        let model = std::env::var("MOD_ENGINE_MODEL")
            .map_err(|_| anyhow::anyhow!("set MOD_ENGINE_MODEL to the vision_model onnx path"))?;
        let enc = embed_onnx::OnnxImageEncoder::new(&model)?;
        let v = enc.encode_image(&std::fs::read(&path)?)?;
        eprintln!("[modsearch-engine] provider={} dim={} norm=1.0", enc.provider(), v.len());
        println!("{}", serde_json::to_string(&v)?);
        return Ok(());
    }

    // A26: offline eval harness. `eval` prints the JSON report; `eval --md` the
    // human table. Pure and store-free, so it runs anywhere.
    if std::env::args().nth(1).as_deref() == Some("eval") {
        let cfg = config::EngineConfig::load()?;
        let report = evalrun::run(&cfg);
        if std::env::args().any(|a| a == "--md") {
            print!("{}", evalrun::render_markdown(&report));
        } else {
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        return Ok(());
    }

    let st = Store::open(&db_path())?;
    st.seed_catalog(&fixtures::catalog())?;
    let catalog = st.load_catalog()?; // A10: the feed serves rows from the store
    let cfg = config::EngineConfig::load()?;
    let encoder = SyntheticEncoder { dim: cfg.vector_dim };
    let index = CatalogIndex::build(&catalog, &encoder);
    let ctx = Ctx::new(cfg, &catalog);

    if std::env::args().nth(1).as_deref() == Some("oneshot") {
        let mut s = String::new();
        std::io::stdin().read_to_string(&mut s)?;
        let raw: RawRequest = serde_json::from_str(&s)?;
        println!("{}", serde_json::to_string_pretty(&dispatch(&catalog, &index, &st, &ctx, &raw))?);
        return Ok(());
    }

    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut r = stdin.lock();
    let mut w = stdout.lock();
    while let Some(bytes) = protocol::read_message(&mut r)? {
        let resp = match serde_json::from_slice::<RawRequest>(&bytes) {
            Ok(raw) => dispatch(&catalog, &index, &st, &ctx, &raw),
            Err(e) => json!({ "ok": false, "error": format!("bad request json: {e}") }),
        };
        let mut out = serde_json::to_vec(&resp)?;
        if out.len() > protocol::MAX_OUT {
            out = serde_json::to_vec(&json!({ "id": resp["id"], "ok": false, "error": "response exceeds 1MB; page it" }))?;
        }
        protocol::write_message(&mut w, &out)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rank::{explore_ratio_for, RankCtx, Reranker};

    struct TestEnv {
        catalog: Vec<Listing>,
        index: CatalogIndex,
        st: Store,
        ctx: Ctx,
    }

    fn test_env() -> TestEnv {
        let st = Store::open(":memory:").unwrap();
        st.seed_catalog(&fixtures::catalog()).unwrap();
        let catalog = st.load_catalog().unwrap();
        let cfg = config::EngineConfig::default();
        let encoder = SyntheticEncoder { dim: cfg.vector_dim };
        let index = CatalogIndex::build(&catalog, &encoder);
        let ctx = Ctx::new(cfg, &catalog);
        TestEnv { catalog, index, st, ctx }
    }

    fn req(json_str: &str) -> RawRequest {
        serde_json::from_str(json_str).unwrap()
    }

    fn call(env: &TestEnv, json_str: &str) -> serde_json::Value {
        dispatch(&env.catalog, &env.index, &env.st, &env.ctx, &req(json_str))
    }

    #[test]
    fn framing_roundtrips() {
        let msg = br#"{"type":"ping"}"#;
        let mut buf: Vec<u8> = Vec::new();
        protocol::write_message(&mut buf, msg).unwrap();
        assert_eq!(&buf[0..4], &(msg.len() as u32).to_le_bytes());
        let mut cur = std::io::Cursor::new(buf);
        let got = protocol::read_message(&mut cur).unwrap().unwrap();
        assert_eq!(got, msg);
        assert!(protocol::read_message(&mut cur).unwrap().is_none());
    }

    #[test]
    fn parse_client_ts_forms() {
        assert_eq!(parse_client_ts("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_client_ts("1970-01-02T00:00:01.250Z"), Some(86_400_000 + 1250));
        assert_eq!(parse_client_ts("1970-01-01T02:00:00+02:00"), Some(0));
        assert_eq!(parse_client_ts("garbage"), None);
    }

    #[test]
    fn feed_returns_items_and_echoes_id() {
        let env = test_env();
        let resp = call(&env, r#"{"id":7,"type":"getFeed","query":{},"hidden":[]}"#);
        assert_eq!(resp["ok"], true);
        assert_eq!(resp["id"], 7);
        assert!(resp["data"].as_array().unwrap().len() > 20);
    }

    #[test]
    fn category_filter_narrows() {
        let env = test_env();
        let resp = call(&env, r#"{"type":"getFeed","query":{"categories":["Footwear"]},"hidden":[]}"#);
        let rows = resp["data"].as_array().unwrap();
        assert!(!rows.is_empty());
        assert!(rows.iter().all(|r| r["category"] == "Footwear"));
    }

    #[test]
    fn measurement_filter_excludes_lacking() {
        let env = test_env();
        let resp = call(&env, r#"{"type":"getFeed","query":{"measures":{"waist":[70,100]}},"hidden":[]}"#);
        let rows = resp["data"].as_array().unwrap();
        assert!(rows.iter().all(|r| r["measurements"]["values"].get("waist").is_some()));
    }

    #[test]
    fn drops_are_all_drops() {
        let env = test_env();
        let resp = call(&env, r#"{"type":"getDrops","hidden":[]}"#);
        for r in resp["data"].as_array().unwrap() {
            let hist = r["priceHistory"].as_array().unwrap();
            let first = hist[0]["price"].as_f64().unwrap();
            let last = r["price"].as_f64().unwrap();
            let pct = (((last - first) / first) * 100.0).round() as i32;
            assert!(pct <= -3, "row pct {pct} not a drop");
        }
    }

    #[test]
    fn get_item_roundtrips() {
        let env = test_env();
        let resp = call(&env, r#"{"type":"getItem","itemId":"it_001"}"#);
        assert_eq!(resp["ok"], true);
        assert_eq!(resp["data"]["id"], "it_001");
        assert!(resp.get("provenance").is_none());
    }

    #[test]
    fn unknown_type_errors() {
        let env = test_env();
        let resp = call(&env, r#"{"type":"nope"}"#);
        assert_eq!(resp["ok"], false);
    }

    // ---- determinism kernel (A29/A30) ----

    const REPLAY: &str = r#"{"id":1,"type":"getFeed","query":{},"hidden":[],"userId":"golden","dayEpoch":20000}"#;

    #[test]
    fn replay_reproduces_exactly() {
        let env = test_env();
        let a = serde_json::to_string(&call(&env, REPLAY)).unwrap();
        let b = serde_json::to_string(&call(&env, REPLAY)).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn explore_differs_across_days_not_runs() {
        let env = test_env();
        let day_a = r#"{"type":"getFeed","query":{},"hidden":[],"userId":"local","dayEpoch":100}"#;
        let day_b = r#"{"type":"getFeed","query":{},"hidden":[],"userId":"local","dayEpoch":101}"#;
        let a1 = serde_json::to_string(&call(&env, day_a)).unwrap();
        let a2 = serde_json::to_string(&call(&env, day_a)).unwrap();
        let b = serde_json::to_string(&call(&env, day_b)).unwrap();
        assert_eq!(a1, a2, "same day must replay identically");
        assert_ne!(a1, b, "different days must explore differently");
    }

    #[test]
    fn explore_records_match_feed_slots() {
        let env = test_env();
        let resp = call(&env, REPLAY);
        let rows = resp["data"].as_array().unwrap();
        let records = resp["provenance"]["explore"].as_array().unwrap();
        assert!(!records.is_empty(), "cold feed must explore");
        // cold user: annealed ratio = explore_max
        let k = (1.0 / explore_ratio_for(0, &env.ctx.cfg)).round() as usize;
        for r in records {
            let slot = r["slot"].as_u64().unwrap() as usize;
            let p = r["propensity"].as_f64().unwrap();
            assert_eq!((slot + 1) % k, 0, "explore slot {slot} not on the k-grid");
            assert!(p > 0.0 && p <= 1.0, "propensity {p} out of range");
            assert_eq!(rows[slot]["id"], r["itemId"], "record does not match the served slot");
        }
        assert_eq!(resp["provenance"]["configHash"], env.ctx.cfg_hash);
        assert_eq!(resp["provenance"]["catalog"], env.ctx.catalog_fp);
        assert_eq!(resp["provenance"]["tasteSnapshot"], "cold");
    }

    #[test]
    fn explicit_sort_is_never_explored() {
        let env = test_env();
        let resp = call(&env, r#"{"type":"getFeed","query":{"sort":"price_asc"},"hidden":[],"dayEpoch":20000}"#);
        assert!(resp["provenance"]["explore"].as_array().unwrap().is_empty());
        let rows = resp["data"].as_array().unwrap();
        let prices: Vec<f64> = rows.iter().map(|r| r["price"].as_f64().unwrap()).collect();
        assert!(prices.windows(2).all(|w| w[0] <= w[1]), "price_asc must be monotone");
    }

    #[test]
    fn golden_top10() {
        // Golden cold-feed head for (query {}, user "golden", day 20000).
        // Any change to the comparator, exploration policy, a ranking default,
        // or the fixtures moves this; update it consciously in the same commit.
        let env = test_env();
        let resp = call(&env, REPLAY);
        let got: Vec<String> = resp["data"].as_array().unwrap().iter().take(10)
            .map(|r| r["id"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(
            got,
            vec!["it_047", "it_048", "it_009", "it_079", "it_046", "it_021", "it_039", "it_014", "it_076", "it_031"]
        );
    }

    #[test]
    fn reranker_seam_swaps_without_handler_edits() {
        struct Reverse;
        impl Reranker for Reverse {
            fn rerank(&self, rows: &mut Vec<Listing>, _ctx: &RankCtx) {
                rows.reverse();
            }
        }
        let env = test_env();
        // explore off so the comparison is exact
        let cfg = config::EngineConfig { explore_max: 0.0, explore_min: 0.0, ..Default::default() };
        let hidden = HashSet::new();
        let q = model::FeedQuery::default();
        let fe = FeedEnv { cfg: &cfg, index: &env.index, taste: None, day_epoch: 20000 };
        let mut rng_a = det::DetRng::new(1);
        let mut rng_b = det::DetRng::new(1);
        let (base, _) = handlers::feed(&env.catalog, &q, &hidden, &fe, &mut rng_a, &IdentityReranker, &SeededFraction);
        let (rev, _) = handlers::feed(&env.catalog, &q, &hidden, &fe, &mut rng_b, &Reverse, &SeededFraction);
        let base_ids: Vec<&str> = base.iter().map(|l| l.id.as_str()).collect();
        let mut rev_ids: Vec<&str> = rev.iter().map(|l| l.id.as_str()).collect();
        rev_ids.reverse();
        assert_eq!(base_ids, rev_ids);
    }

    // ---- A10/A13: persistence + learning ----

    #[test]
    fn feed_learns_from_feedback() {
        let env = test_env();
        let cold = call(&env, REPLAY);
        let cold_ids: Vec<String> = cold["data"].as_array().unwrap().iter()
            .map(|r| r["id"].as_str().unwrap().to_string()).collect();

        // like three items of one brand (explicit ts: deterministic replay)
        let brand = env.catalog.iter().find(|l| l.id == cold_ids[0]).unwrap().brand.clone();
        let liked: Vec<String> = env.catalog.iter()
            .filter(|l| l.brand == brand).take(3).map(|l| l.id.clone()).collect();
        assert!(liked.len() >= 2, "fixture must carry repeated brands");
        for (i, id) in liked.iter().enumerate() {
            let resp = call(&env, &format!(
                r#"{{"type":"recordFeedback","itemId":"{id}","feedbackKind":"like","userId":"golden","ts":{}}}"#,
                1_700_000_000_000i64 + i as i64 * 60_000
            ));
            assert_eq!(resp["ok"], true, "{resp}");
            assert_eq!(resp["data"]["applied"], true);
        }

        let warm = call(&env, REPLAY);
        assert_eq!(warm["provenance"]["nInteractions"], 3);
        assert_ne!(warm["provenance"]["tasteSnapshot"], "cold");
        let warm_ids: Vec<String> = warm["data"].as_array().unwrap().iter()
            .map(|r| r["id"].as_str().unwrap().to_string()).collect();
        assert_ne!(cold_ids, warm_ids, "taste must reorder the feed");

        // same-brand items (excluding the liked ones) must move up on average
        let mean_rank = |ids: &[String]| -> f64 {
            let ranks: Vec<f64> = ids.iter().enumerate()
                .filter(|(_, id)| {
                    let l = &env.catalog[env.index.by_id[id.as_str()]];
                    l.brand == brand && !liked.contains(id)
                })
                .map(|(pos, _)| pos as f64)
                .collect();
            ranks.iter().sum::<f64>() / ranks.len().max(1) as f64
        };
        assert!(
            mean_rank(&warm_ids) < mean_rank(&cold_ids),
            "liked brand must rank higher after feedback"
        );

        // and the taste ranking itself replays deterministically
        let warm2 = call(&env, REPLAY);
        assert_eq!(
            serde_json::to_string(&warm).unwrap(),
            serde_json::to_string(&warm2).unwrap()
        );
    }

    #[test]
    fn save_feedback_persists_into_saved() {
        let env = test_env();
        call(&env, r#"{"type":"recordFeedback","itemId":"it_005","feedbackKind":"save","userId":"golden","ts":1700000000000}"#);
        let resp = call(&env, r#"{"type":"getSaved","userId":"golden","savedIds":[]}"#);
        let ids: Vec<&str> = resp["data"].as_array().unwrap().iter()
            .map(|r| r["id"].as_str().unwrap()).collect();
        assert_eq!(ids, vec!["it_005"]);
    }

    #[test]
    fn hide_voids_the_open_episode_and_records() {
        let env = test_env();
        // open an episode with a pending save on it_010
        call(&env, r#"{"type":"recordEvents","userId":"golden","events":[
            {"event_id":"h1","type":"save","item_id":"it_010","session_id":"s1234567","device_id":"d1234567","client_ts":"2026-09-01T10:00:00Z"}
        ]}"#);
        assert!(env.st.get_episode("u:golden", "it_010").unwrap().is_some());
        // hide voids it: the pending positive never scores, taste stays cold
        let resp = call(&env, r#"{"type":"recordFeedback","itemId":"it_010","feedbackKind":"hide","userId":"golden","ts":1756720800000}"#);
        assert_eq!(resp["ok"], true, "{resp}");
        assert_eq!(resp["data"]["applied"], false);
        assert!(env.st.get_episode("u:golden", "it_010").unwrap().is_none());
        assert!(env.st.taste_fingerprint("u:golden").unwrap().is_none(), "hide must not move taste");
    }

    #[test]
    fn events_fold_into_episodes_and_flush_on_impression_end() {
        let env = test_env();
        let batch = r#"{"type":"recordEvents","userId":"golden","events":[
            {"event_id":"e1","type":"click_detail","item_id":"it_020","session_id":"s1234567","device_id":"d1234567","client_ts":"2026-09-01T10:00:00Z"},
            {"event_id":"e2","type":"save","item_id":"it_020","session_id":"s1234567","device_id":"d1234567","client_ts":"2026-09-01T10:00:05Z"},
            {"event_id":"e3","type":"impression_end","item_id":"it_020","dwell_ms":12000,"session_id":"s1234567","device_id":"d1234567","client_ts":"2026-09-01T10:00:12Z"}
        ]}"#;
        let resp = call(&env, batch);
        assert_eq!(resp["ok"], true, "{resp}");
        assert_eq!(resp["data"]["recorded"], 3);
        assert_eq!(resp["data"]["flushed"], 1);
        // replaying the identical batch is a no-op (event_id dedupe)
        let again = call(&env, batch);
        assert_eq!(again["data"]["recorded"], 0);
        assert_eq!(again["data"]["flushed"], 0);
        // the episode scored R = w(12s) + click + save = 2.16949 and moved taste
        let meta = env.st.get_meta("u:golden").unwrap().unwrap();
        assert_eq!(meta.n_interactions, 1);
        let fp = env.st.taste_fingerprint("u:golden").unwrap();
        assert!(fp.is_some());

        // determinism: the same stream against a fresh store lands the same state
        let env2 = test_env();
        call(&env2, batch);
        assert_eq!(fp, env2.st.taste_fingerprint("u:golden").unwrap());
    }

    #[test]
    fn stale_episode_flushes_on_idle_horizon() {
        let env = test_env();
        let first = r#"{"type":"recordEvents","userId":"golden","events":[
            {"event_id":"a1","type":"like","item_id":"it_030","session_id":"s1234567","device_id":"d1234567","client_ts":"2026-09-01T10:00:00Z"}
        ]}"#;
        let resp = call(&env, first);
        assert_eq!(resp["data"]["flushed"], 0, "episode stays open inside the idle horizon");
        // a later batch (2 minutes on) for another item pushes the idle sweep past it
        let second = r#"{"type":"recordEvents","userId":"golden","events":[
            {"event_id":"a2","type":"like","item_id":"it_031","session_id":"s1234567","device_id":"d1234567","client_ts":"2026-09-01T10:02:00Z"}
        ]}"#;
        let resp2 = call(&env, second);
        assert_eq!(resp2["data"]["flushed"], 1, "idle episode for it_030 must flush");
        assert_eq!(env.st.get_meta("u:golden").unwrap().unwrap().n_interactions, 1);
    }
}
