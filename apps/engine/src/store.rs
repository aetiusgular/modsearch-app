//! A10: local persistence. SQLite via rusqlite (bundled), user_version
//! migrations, one repository struct. DuckDB is deliberately deferred: the
//! analytics reads (price history) sit behind the small `AnalyticsStore` trait
//! at the bottom so a columnar store can drop in when volumes demand it.
//!
//! Determinism note: this module never reads the clock. Every timestamp is
//! handed in by the caller (event `client_ts`, request `ts`), so state
//! transitions are a pure function of the request stream.

use crate::model::Listing;
use anyhow::{anyhow, Context, Result};
use rusqlite::{params, Connection, OptionalExtension};

/// Float32 little-endian vector codec (port of profiles/codec.py). The blob
/// layout is the parity contract with the engine: dim x 4 bytes, LE, finite.
pub fn vec_to_bytes(v: &[f32]) -> Result<Vec<u8>> {
    if v.iter().any(|x| !x.is_finite()) {
        return Err(anyhow!("vector contains non-finite values"));
    }
    let mut out = Vec::with_capacity(v.len() * 4);
    for x in v {
        out.extend_from_slice(&x.to_le_bytes());
    }
    Ok(out)
}

pub fn bytes_to_vec(blob: &[u8], dim: usize) -> Result<Vec<f32>> {
    if blob.len() != dim * 4 {
        return Err(anyhow!("bad blob length: expected {} got {}", dim * 4, blob.len()));
    }
    let mut out = Vec::with_capacity(dim);
    for chunk in blob.chunks_exact(4) {
        let x = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        if !x.is_finite() {
            return Err(anyhow!("blob contains non-finite values"));
        }
        out.push(x);
    }
    Ok(out)
}

/// (opened_ms, last_ms, dwell_ms, kinds_csv, event_ids_csv) as stored.
pub type EpisodeRow = (i64, i64, Option<i64>, String, String);

/// Stored profile vector + shared meta for one user.
pub struct ProfileMeta {
    pub last_update_ms: i64,
    pub n_interactions: i64,
}

pub struct Store {
    conn: Connection,
}

const SCHEMA_VERSION: i64 = 1;

impl Store {
    /// Open (or create) the store at `path`; ":memory:" for tests.
    pub fn open(path: &str) -> Result<Self> {
        let conn = if path == ":memory:" {
            Connection::open_in_memory()?
        } else {
            if let Some(dir) = std::path::Path::new(path).parent() {
                if !dir.as_os_str().is_empty() {
                    std::fs::create_dir_all(dir).context("creating db directory")?;
                }
            }
            Connection::open(path)?
        };
        conn.pragma_update(None, "journal_mode", "WAL").ok(); // in-memory has no WAL
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let store = Self { conn };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> Result<()> {
        let v: i64 = self.conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
        if v < 1 {
            self.conn.execute_batch(
                "BEGIN;
                 CREATE TABLE listings (
                   id      TEXT PRIMARY KEY,
                   json    TEXT NOT NULL
                 );
                 CREATE TABLE events (
                   event_id TEXT PRIMARY KEY,
                   user_key TEXT NOT NULL,
                   item_id  TEXT,
                   kind     TEXT NOT NULL,
                   ts_ms    INTEGER NOT NULL,
                   json     TEXT NOT NULL
                 );
                 CREATE TABLE episodes (
                   user_key  TEXT NOT NULL,
                   item_id   TEXT NOT NULL,
                   opened_ms INTEGER NOT NULL,
                   last_ms   INTEGER NOT NULL,
                   dwell_ms  INTEGER,
                   kinds     TEXT NOT NULL,
                   event_ids TEXT NOT NULL,
                   PRIMARY KEY (user_key, item_id)
                 );
                 CREATE TABLE interactions (
                   id        INTEGER PRIMARY KEY AUTOINCREMENT,
                   user_key  TEXT NOT NULL,
                   item_id   TEXT NOT NULL,
                   r_score   REAL NOT NULL,
                   w_implicit REAL NOT NULL,
                   ts_ms     INTEGER NOT NULL
                 );
                 CREATE TABLE profiles (
                   user_key TEXT NOT NULL,
                   space    TEXT NOT NULL,
                   window   TEXT NOT NULL,
                   vec      BLOB NOT NULL,
                   PRIMARY KEY (user_key, space, window)
                 );
                 CREATE TABLE profile_meta (
                   user_key       TEXT PRIMARY KEY,
                   last_update_ms INTEGER NOT NULL,
                   n_interactions INTEGER NOT NULL
                 );
                 CREATE TABLE graph_history (
                   user_key TEXT NOT NULL,
                   item_id  TEXT NOT NULL,
                   last_ms  INTEGER NOT NULL,
                   PRIMARY KEY (user_key, item_id)
                 );
                 CREATE TABLE saved (
                   user_key TEXT NOT NULL,
                   item_id  TEXT NOT NULL,
                   ts_ms    INTEGER NOT NULL,
                   PRIMARY KEY (user_key, item_id)
                 );
                 CREATE TABLE feedback (
                   id       INTEGER PRIMARY KEY AUTOINCREMENT,
                   user_key TEXT NOT NULL,
                   item_id  TEXT NOT NULL,
                   kind     TEXT NOT NULL,
                   ts_ms    INTEGER NOT NULL
                 );
                 CREATE TABLE price_log (
                   item_id TEXT NOT NULL,
                   t       TEXT NOT NULL,
                   price   REAL NOT NULL,
                   PRIMARY KEY (item_id, t)
                 );
                 CREATE TABLE settings (
                   key   TEXT PRIMARY KEY,
                   value TEXT NOT NULL
                 );
                 PRAGMA user_version = 1;
                 COMMIT;",
            )?;
        }
        let now: i64 = self.conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
        if now != SCHEMA_VERSION {
            return Err(anyhow!("unsupported schema version {now}"));
        }
        Ok(())
    }

    // ---- catalog ----

    /// Seed/refresh the catalog snapshot and price log from listings.
    pub fn seed_catalog(&self, listings: &[Listing]) -> Result<()> {
        self.conn.execute_batch("BEGIN IMMEDIATE;")?;
        {
            let mut up = self
                .conn
                .prepare("INSERT OR REPLACE INTO listings (id, json) VALUES (?1, ?2)")?;
            let mut price = self.conn.prepare(
                "INSERT OR IGNORE INTO price_log (item_id, t, price) VALUES (?1, ?2, ?3)",
            )?;
            for l in listings {
                up.execute(params![l.id, serde_json::to_string(l)?])?;
                for p in &l.price_history {
                    price.execute(params![l.id, p.t, p.price])?;
                }
            }
        }
        self.conn.execute_batch("COMMIT;")?;
        Ok(())
    }

    /// Read the full catalog back (A10 AC: the feed serves rows from the store).
    pub fn load_catalog(&self) -> Result<Vec<Listing>> {
        let mut stmt = self.conn.prepare("SELECT json FROM listings ORDER BY id")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        let mut out = Vec::new();
        for j in rows {
            out.push(serde_json::from_str(&j?)?);
        }
        Ok(out)
    }

    // ---- events / episodes ----

    /// Insert one raw event; returns false when event_id was already stored
    /// (at-least-once replay safety, mirrors the engine's dedupe).
    pub fn insert_event(
        &self,
        event_id: &str,
        user_key: &str,
        item_id: Option<&str>,
        kind: &str,
        ts_ms: i64,
        json: &str,
    ) -> Result<bool> {
        let n = self.conn.execute(
            "INSERT OR IGNORE INTO events (event_id, user_key, item_id, kind, ts_ms, json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![event_id, user_key, item_id, kind, ts_ms, json],
        )?;
        Ok(n > 0)
    }

    pub fn get_episode(&self, user_key: &str, item_id: &str) -> Result<Option<EpisodeRow>> {
        Ok(self
            .conn
            .query_row(
                "SELECT opened_ms, last_ms, dwell_ms, kinds, event_ids
                 FROM episodes WHERE user_key = ?1 AND item_id = ?2",
                params![user_key, item_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .optional()?)
    }

    pub fn put_episode(
        &self,
        user_key: &str,
        item_id: &str,
        opened_ms: i64,
        last_ms: i64,
        dwell_ms: Option<i64>,
        kinds: &str,
        event_ids: &str,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO episodes (user_key, item_id, opened_ms, last_ms, dwell_ms, kinds, event_ids)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![user_key, item_id, opened_ms, last_ms, dwell_ms, kinds, event_ids],
        )?;
        Ok(())
    }

    pub fn delete_episode(&self, user_key: &str, item_id: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM episodes WHERE user_key = ?1 AND item_id = ?2",
            params![user_key, item_id],
        )?;
        Ok(())
    }

    /// Episodes idle past the flush horizon relative to `now_ms` (event time).
    pub fn idle_episodes(&self, now_ms: i64, idle_ms: i64) -> Result<Vec<(String, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT user_key, item_id FROM episodes WHERE ?1 - last_ms > ?2 ORDER BY user_key, item_id",
        )?;
        let rows = stmt.query_map(params![now_ms, idle_ms], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn record_interaction(
        &self,
        user_key: &str,
        item_id: &str,
        r_score: f64,
        w_implicit: f64,
        ts_ms: i64,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO interactions (user_key, item_id, r_score, w_implicit, ts_ms)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![user_key, item_id, r_score, w_implicit, ts_ms],
        )?;
        Ok(())
    }

    // ---- profiles ----

    pub fn get_profile(&self, user_key: &str, space: &str, window: &str, dim: usize) -> Result<Option<Vec<f32>>> {
        let blob: Option<Vec<u8>> = self
            .conn
            .query_row(
                "SELECT vec FROM profiles WHERE user_key = ?1 AND space = ?2 AND window = ?3",
                params![user_key, space, window],
                |r| r.get(0),
            )
            .optional()?;
        blob.map(|b| bytes_to_vec(&b, dim)).transpose()
    }

    pub fn get_meta(&self, user_key: &str) -> Result<Option<ProfileMeta>> {
        Ok(self
            .conn
            .query_row(
                "SELECT last_update_ms, n_interactions FROM profile_meta WHERE user_key = ?1",
                params![user_key],
                |r| {
                    Ok(ProfileMeta { last_update_ms: r.get(0)?, n_interactions: r.get(1)? })
                },
            )
            .optional()?)
    }

    /// Write all (space, window) vectors plus meta in one transaction, mirroring
    /// the engine's atomic Lua update (windows never desynchronize).
    pub fn put_profiles(
        &self,
        user_key: &str,
        vectors: &[(&str, &str, Vec<f32>)],
        last_update_ms: i64,
    ) -> Result<i64> {
        self.conn.execute_batch("BEGIN IMMEDIATE;")?;
        {
            let mut up = self.conn.prepare(
                "INSERT OR REPLACE INTO profiles (user_key, space, window, vec) VALUES (?1, ?2, ?3, ?4)",
            )?;
            for (space, window, vec) in vectors {
                up.execute(params![user_key, space, window, vec_to_bytes(vec)?])?;
            }
        }
        self.conn.execute(
            "INSERT INTO profile_meta (user_key, last_update_ms, n_interactions)
             VALUES (?1, ?2, 1)
             ON CONFLICT(user_key) DO UPDATE SET
               last_update_ms = excluded.last_update_ms,
               n_interactions = n_interactions + 1",
            params![user_key, last_update_ms],
        )?;
        let n: i64 = self.conn.query_row(
            "SELECT n_interactions FROM profile_meta WHERE user_key = ?1",
            params![user_key],
            |r| r.get(0),
        )?;
        self.conn.execute_batch("COMMIT;")?;
        Ok(n)
    }

    /// Stable fingerprint of a user's taste state, for provenance.
    pub fn taste_fingerprint(&self, user_key: &str) -> Result<Option<String>> {
        let meta = match self.get_meta(user_key)? {
            None => return Ok(None),
            Some(m) => m,
        };
        let mut bytes: Vec<u8> = Vec::new();
        bytes.extend_from_slice(&meta.last_update_ms.to_le_bytes());
        bytes.extend_from_slice(&meta.n_interactions.to_le_bytes());
        let mut stmt = self.conn.prepare(
            "SELECT space, window, vec FROM profiles WHERE user_key = ?1 ORDER BY space, window",
        )?;
        let rows = stmt.query_map(params![user_key], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, Vec<u8>>(2)?))
        })?;
        for row in rows {
            let (space, window, blob) = row?;
            bytes.extend_from_slice(space.as_bytes());
            bytes.extend_from_slice(window.as_bytes());
            bytes.extend_from_slice(&blob);
        }
        Ok(Some(format!("{:016x}", crate::det::fnv1a(&bytes))))
    }

    // ---- graph history / saved / feedback ----

    /// Dedupe-refresh recency (LREM/LPUSH/LTRIM equivalent): re-interaction
    /// bumps last_ms; reads take the newest `k`.
    pub fn record_history(&self, user_key: &str, item_id: &str, ts_ms: i64) -> Result<()> {
        self.conn.execute(
            "INSERT INTO graph_history (user_key, item_id, last_ms) VALUES (?1, ?2, ?3)
             ON CONFLICT(user_key, item_id) DO UPDATE SET last_ms = MAX(last_ms, excluded.last_ms)",
            params![user_key, item_id, ts_ms],
        )?;
        Ok(())
    }

    pub fn recent_history(&self, user_key: &str, k: usize) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT item_id FROM graph_history WHERE user_key = ?1
             ORDER BY last_ms DESC, item_id ASC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![user_key, k as i64], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn record_feedback(&self, user_key: &str, item_id: &str, kind: &str, ts_ms: i64) -> Result<()> {
        self.conn.execute(
            "INSERT INTO feedback (user_key, item_id, kind, ts_ms) VALUES (?1, ?2, ?3, ?4)",
            params![user_key, item_id, kind, ts_ms],
        )?;
        if kind == "save" {
            self.conn.execute(
                "INSERT OR IGNORE INTO saved (user_key, item_id, ts_ms) VALUES (?1, ?2, ?3)",
                params![user_key, item_id, ts_ms],
            )?;
        }
        Ok(())
    }

    pub fn saved_ids(&self, user_key: &str) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT item_id FROM saved WHERE user_key = ?1 ORDER BY ts_ms DESC, item_id ASC",
        )?;
        let rows = stmt.query_map(params![user_key], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }
}

/// The analytics seam (A10 decision, 2026-09): SQLite first, DuckDB drops in
/// behind this trait when columnar scans are actually needed. Unused outside
/// tests today by design; A25/analytics consumers call through it.
#[allow(dead_code)]
pub trait AnalyticsStore {
    fn price_history(&self, item_id: &str) -> Result<Vec<(String, f64)>>;
}

impl AnalyticsStore for Store {
    fn price_history(&self, item_id: &str) -> Result<Vec<(String, f64)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT t, price FROM price_log WHERE item_id = ?1 ORDER BY t ASC")?;
        let rows = stmt.query_map(params![item_id], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;

    #[test]
    fn codec_roundtrips_and_guards() {
        let v = vec![0.25f32, -1.5, 3.75];
        let b = vec_to_bytes(&v).unwrap();
        assert_eq!(b.len(), 12);
        assert_eq!(bytes_to_vec(&b, 3).unwrap(), v);
        assert!(bytes_to_vec(&b, 4).is_err());
        assert!(vec_to_bytes(&[f32::NAN]).is_err());
    }

    #[test]
    fn catalog_seeds_and_loads() {
        let s = Store::open(":memory:").unwrap();
        let cat = fixtures::catalog();
        s.seed_catalog(&cat).unwrap();
        let loaded = s.load_catalog().unwrap();
        assert_eq!(loaded.len(), cat.len());
        let hist = s.price_history(&cat[0].id).unwrap();
        assert_eq!(hist.len(), cat[0].price_history.len());
    }

    #[test]
    fn event_dedupe_is_idempotent() {
        let s = Store::open(":memory:").unwrap();
        assert!(s.insert_event("e1", "u:x", Some("it_001"), "like", 1000, "{}").unwrap());
        assert!(!s.insert_event("e1", "u:x", Some("it_001"), "like", 1000, "{}").unwrap());
    }

    #[test]
    fn profiles_write_atomically_and_count() {
        let s = Store::open(":memory:").unwrap();
        let v = vec![1.0f32; 8];
        let n1 = s
            .put_profiles("u:x", &[("clip_base", "short", v.clone()), ("clip_base", "long", v.clone())], 1000)
            .unwrap();
        let n2 = s
            .put_profiles("u:x", &[("clip_base", "short", v.clone()), ("clip_base", "long", v)], 2000)
            .unwrap();
        assert_eq!((n1, n2), (1, 2));
        assert!(s.get_profile("u:x", "clip_base", "short", 8).unwrap().is_some());
        assert_eq!(s.get_meta("u:x").unwrap().unwrap().last_update_ms, 2000);
        assert!(s.taste_fingerprint("u:x").unwrap().is_some());
        assert!(s.taste_fingerprint("u:nobody").unwrap().is_none());
    }

    #[test]
    fn data_survives_reopen() {
        let path = std::env::temp_dir().join(format!("aura-test-{}.db", std::process::id()));
        let p = path.to_str().unwrap().to_string();
        let _ = std::fs::remove_file(&p);
        {
            let s = Store::open(&p).unwrap();
            s.record_feedback("u:x", "it_001", "save", 1234).unwrap();
        }
        {
            let s = Store::open(&p).unwrap();
            assert_eq!(s.saved_ids("u:x").unwrap(), vec!["it_001".to_string()]);
        }
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn history_dedupe_refreshes_recency() {
        let s = Store::open(":memory:").unwrap();
        s.record_history("u:x", "a", 1).unwrap();
        s.record_history("u:x", "b", 2).unwrap();
        s.record_history("u:x", "a", 3).unwrap();
        assert_eq!(s.recent_history("u:x", 10).unwrap(), vec!["a".to_string(), "b".to_string()]);
        assert_eq!(s.recent_history("u:x", 1).unwrap(), vec!["a".to_string()]);
    }
}
