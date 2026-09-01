//! Native-messaging framing + request envelope.
//!
//! Chrome/Firefox native messaging frames each JSON message with a 4-byte length
//! prefix in the OS native byte order (little-endian on our targets). Host->browser
//! messages are capped at 1 MB, so large feeds are paged rather than returned whole.

use crate::model::FeedQuery;
use serde::Deserialize;
use std::io::{self, Read, Write};

pub const MAX_OUT: usize = 1024 * 1024; // 1 MB host->browser cap

/// Read one framed message. Returns Ok(None) at clean EOF (browser closed the port).
pub fn read_message<R: Read>(r: &mut R) -> io::Result<Option<Vec<u8>>> {
    let mut len_buf = [0u8; 4];
    match r.read_exact(&mut len_buf) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let len = u32::from_le_bytes(len_buf) as usize;
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf)?;
    Ok(Some(buf))
}

/// Write one framed message.
pub fn write_message<W: Write>(w: &mut W, payload: &[u8]) -> io::Result<()> {
    let len = payload.len() as u32;
    w.write_all(&len.to_le_bytes())?;
    w.write_all(payload)?;
    w.flush()
}

/// The request envelope. `type` selects the handler; the rest are optional fields
/// the various requests use. Kept flat so the JS side builds a plain object.
#[derive(Deserialize, Default)]
pub struct RawRequest {
    pub id: Option<u64>,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub query: FeedQuery,
    #[serde(default)]
    pub hidden: Vec<String>,
    #[serde(default, rename = "itemId")]
    pub item_id: Option<String>,
    #[serde(default, rename = "savedIds")]
    pub saved_ids: Vec<String>,
    #[serde(default, rename = "feedbackKind")]
    pub feedback_kind: Option<String>,
    #[serde(default)]
    pub events: Vec<serde_json::Value>,
}
