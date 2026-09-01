//! AuraSearch headless engine service (A9).
//!
//! Default mode is the framed native-messaging loop over stdin/stdout that the
//! browser extension's host connects to. `aurasearch-engine oneshot` reads one
//! unframed JSON request from stdin and prints the response, for local testing.
//! ADR-0002: this is a standalone service, not a Tauri child. A10+ add the real
//! stores, vector index, graph, and taste; A20 splits it into a persistent
//! service + native-messaging proxy.

mod fixtures;
mod handlers;
mod model;
mod protocol;

use model::Listing;
use protocol::RawRequest;
use serde_json::json;
use std::collections::HashSet;
use std::io::Read;

fn dispatch(catalog: &[Listing], raw: &RawRequest) -> serde_json::Value {
    let hidden: HashSet<String> = raw.hidden.iter().cloned().collect();
    let saved_ids: HashSet<String> = raw.saved_ids.iter().cloned().collect();

    let data: Result<serde_json::Value, String> = match raw.kind.as_str() {
        "ping" => Ok(json!({ "pong": true, "version": env!("CARGO_PKG_VERSION") })),
        "getFeed" | "search" => Ok(json!(handlers::feed(catalog, &raw.query, &hidden))),
        "getItem" => match raw.item_id.as_deref() {
            Some(id) => Ok(json!(handlers::get_item(catalog, id))),
            None => Err("getItem requires itemId".into()),
        },
        "moreLikeThis" => match raw.item_id.as_deref() {
            Some(id) => Ok(json!(handlers::more_like(catalog, id, &hidden))),
            None => Err("moreLikeThis requires itemId".into()),
        },
        "getFacets" => Ok(handlers::facets(catalog)),
        "getSaved" => Ok(json!(handlers::saved(catalog, &saved_ids))),
        "getDrops" => Ok(json!(handlers::drops(catalog, &hidden))),
        // A13 turns these into real taste updates; for now acknowledge.
        "recordFeedback" => Ok(json!({ "recorded": true, "itemId": raw.item_id, "kind": raw.feedback_kind })),
        "recordEvents" => Ok(json!({ "recorded": raw.events.len() })),
        other => Err(format!("unknown request type: {other}")),
    };

    match data {
        Ok(d) => json!({ "id": raw.id, "ok": true, "type": raw.kind, "data": d }),
        Err(e) => json!({ "id": raw.id, "ok": false, "type": raw.kind, "error": e }),
    }
}

fn main() -> anyhow::Result<()> {
    let catalog = fixtures::catalog();

    if std::env::args().nth(1).as_deref() == Some("oneshot") {
        let mut s = String::new();
        std::io::stdin().read_to_string(&mut s)?;
        let raw: RawRequest = serde_json::from_str(&s)?;
        println!("{}", serde_json::to_string_pretty(&dispatch(&catalog, &raw))?);
        return Ok(());
    }

    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut r = stdin.lock();
    let mut w = stdout.lock();
    while let Some(bytes) = protocol::read_message(&mut r)? {
        let resp = match serde_json::from_slice::<RawRequest>(&bytes) {
            Ok(raw) => dispatch(&catalog, &raw),
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

    fn req(json_str: &str) -> RawRequest {
        serde_json::from_str(json_str).unwrap()
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
        // clean EOF yields None
        assert!(protocol::read_message(&mut cur).unwrap().is_none());
    }

    #[test]
    fn feed_returns_items_and_echoes_id() {
        let cat = fixtures::catalog();
        let resp = dispatch(&cat, &req(r#"{"id":7,"type":"getFeed","query":{},"hidden":[]}"#));
        assert_eq!(resp["ok"], true);
        assert_eq!(resp["id"], 7);
        assert!(resp["data"].as_array().unwrap().len() > 20);
    }

    #[test]
    fn category_filter_narrows() {
        let cat = fixtures::catalog();
        let resp = dispatch(&cat, &req(r#"{"type":"getFeed","query":{"categories":["Footwear"]},"hidden":[]}"#));
        let rows = resp["data"].as_array().unwrap();
        assert!(rows.iter().all(|r| r["category"] == "Footwear"));
    }

    #[test]
    fn measurement_filter_excludes_lacking() {
        let cat = fixtures::catalog();
        // filtering on waist should exclude tops/knitwear/outerwear that have no waist value
        let resp = dispatch(&cat, &req(r#"{"type":"getFeed","query":{"measures":{"waist":[70,100]}},"hidden":[]}"#));
        let rows = resp["data"].as_array().unwrap();
        assert!(rows.iter().all(|r| r["measurements"]["values"].get("waist").is_some()));
    }

    #[test]
    fn drops_are_all_drops() {
        let cat = fixtures::catalog();
        let resp = dispatch(&cat, &req(r#"{"type":"getDrops","hidden":[]}"#));
        for r in resp["data"].as_array().unwrap() {
            let hist = r["priceHistory"].as_array().unwrap();
            let first = hist[0]["price"].as_f64().unwrap();
            let last = r["price"].as_f64().unwrap();
            // matches Listing::drop_pct: rounded percent must be a real cut (<= -3)
            let pct = (((last - first) / first) * 100.0).round() as i32;
            assert!(pct <= -3, "row pct {pct} not a drop");
        }
    }

    #[test]
    fn get_item_roundtrips() {
        let cat = fixtures::catalog();
        let resp = dispatch(&cat, &req(r#"{"type":"getItem","itemId":"it_001"}"#));
        assert_eq!(resp["ok"], true);
        assert_eq!(resp["data"]["id"], "it_001");
    }

    #[test]
    fn unknown_type_errors() {
        let cat = fixtures::catalog();
        let resp = dispatch(&cat, &req(r#"{"type":"nope"}"#));
        assert_eq!(resp["ok"], false);
    }
}
