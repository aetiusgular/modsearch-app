// A15 capture core. Pure logic with no chrome/DOM APIs, so it is unit-testable
// in node (module.exports) and reused by the content script (globalThis.__msCapture,
// since manifest content scripts share one isolated world). It measures per-item
// visible dwell across visibility changes and builds contract-shaped
// TelemetryEvents (envelope + impression_start / heartbeat / impression_end /
// click_detail). The engine computes the dwell reward w(d)=0.3*ln(1+d); here we
// only measure dwell_ms accurately, which is what the episode aggregation needs.
// See packages/contract/schema/telemetry-event.schema.json.
(function (root) {
  "use strict";

  const SCHEMA_VERSION = 1;

  function clamp01(x) {
    if (typeof x !== "number" || Number.isNaN(x)) return 0;
    return x < 0 ? 0 : x > 1 ? 1 : x;
  }

  // The fields shared by every event (_EventBase). `ids` supplies session/device,
  // a uuid factory, and the user id; `nowMs` is the injected clock so the whole
  // thing is deterministic under test.
  function envelope(type, ids, nowMs) {
    return {
      event_id: ids.uuid(),
      session_id: ids.sessionId,
      device_id: ids.deviceId,
      user_id: ids.userId == null ? null : ids.userId,
      client_ts: new Date(nowMs).toISOString(),
      schema_version: SCHEMA_VERSION,
      type: type,
    };
  }

  // Tracks one item's visible dwell. start() emits impression_start once;
  // onVisible/onHidden accumulate only the wall time the item was actually
  // foregrounded; end() emits impression_end with the accumulated dwell_ms.
  // Heartbeats are liveness pings the engine uses to keep an episode open.
  class ItemDwell {
    constructor(itemId, ids, emit) {
      this.itemId = itemId;
      this.ids = ids;
      this.emit = emit;
      this.visibleSince = null;
      this.accumMs = 0;
      this.maxViewport = 0;
      this.started = false;
    }
    start(nowMs, viewportPct) {
      if (this.started) return;
      this.started = true;
      this.visibleSince = nowMs;
      this.maxViewport = Math.max(this.maxViewport, clamp01(viewportPct == null ? 1 : viewportPct));
      const e = envelope("impression_start", this.ids, nowMs);
      e.item_id = this.itemId;
      e.position = 0;
      e.viewport_pct = clamp01(viewportPct == null ? 1 : viewportPct);
      this.emit(e);
    }
    onVisible(nowMs) {
      if (this.started && this.visibleSince == null) this.visibleSince = nowMs;
    }
    onHidden(nowMs) {
      if (this.visibleSince != null) {
        this.accumMs += Math.max(0, nowMs - this.visibleSince);
        this.visibleSince = null;
      }
    }
    heartbeat(nowMs, visible) {
      const e = envelope("heartbeat", this.ids, nowMs);
      e.item_id = this.itemId;
      e.visible = !!visible;
      this.emit(e);
    }
    dwellMs(nowMs) {
      let d = this.accumMs;
      if (this.visibleSince != null) d += Math.max(0, nowMs - this.visibleSince);
      return d;
    }
    end(nowMs) {
      if (!this.started) return;
      this.onHidden(nowMs);
      const e = envelope("impression_end", this.ids, nowMs);
      e.item_id = this.itemId;
      e.dwell_ms = Math.round(this.accumMs);
      e.max_viewport_pct = clamp01(this.maxViewport);
      this.emit(e);
      this.started = false;
    }
  }

  function clickDetail(itemId, ids, nowMs, source) {
    const e = envelope("click_detail", ids, nowMs);
    e.item_id = itemId;
    e.source = source || "external";
    return e;
  }

  const api = { SCHEMA_VERSION, clamp01, envelope, ItemDwell, clickDetail };
  if (typeof module !== "undefined" && module.exports) module.exports = api;
  root.__msCapture = api;
})(typeof globalThis !== "undefined" ? globalThis : this);
