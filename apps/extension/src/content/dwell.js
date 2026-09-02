// A15 per-tab content script. Consent-gated. Measures dwell and interaction on
// the foreground product page and forwards contract TelemetryEvents to the engine
// through the service-worker relay (type "recordEvents"). It lives as long as the
// page, so it (not the ephemeral MV3 worker) owns dwell timing via
// visibilitychange + heartbeats + pagehide. Foreground-only: no background or
// paginated queries, matching the Lane-B constraints. See MODSEARCH_PRD_v3.md
// Sections 4 and 9.
//
// capture-core.js and consent.js are listed before this file in the manifest, so
// they share this content script's isolated world via globalThis.
(async function () {
  "use strict";

  const C = globalThis.__msCapture;
  const Consent = globalThis.__msConsent;
  if (!C || !Consent) return;

  // GATE: nothing below runs until the user has opted in on the consent screen.
  // No listener is attached, no id is read, no event is built before this passes.
  if (!(await Consent.captureAllowed())) return;

  // Only capture on a recognizable product page, and only an item that matches a
  // catalog id shape the engine can learn from (shopify:<domain>:<product_id>).
  const itemId = detectShopifyItemId();
  if (!itemId) return;

  const ids = await sessionIds();
  const queue = [];
  const flush = () => {
    if (!queue.length) return;
    const events = queue.splice(0, queue.length);
    try {
      chrome.runtime.sendMessage({ id: "cap-" + ids.uuid(), type: "recordEvents", userId: ids.userId, events: events });
    } catch (_e) {
      // Worker asleep or gone; drop this batch. The next flush (heartbeat or
      // pagehide) retries, and the engine dedupes by event_id regardless.
    }
  };
  const emit = (e) => {
    queue.push(e);
    if (queue.length >= 12) flush();
  };

  const dwell = new C.ItemDwell(itemId, ids, emit);
  const visible = () => document.visibilityState === "visible";

  dwell.start(Date.now(), 1);
  if (!visible()) dwell.onHidden(Date.now());

  document.addEventListener("visibilitychange", () => {
    const now = Date.now();
    if (visible()) dwell.onVisible(now);
    else {
      dwell.onHidden(now);
      flush(); // persist accrued dwell whenever the tab is backgrounded
    }
    dwell.heartbeat(now, visible());
  });

  // Periodic liveness + flush while the page is open. 15s keeps well under the
  // engine's 60s idle-flush horizon, so an open episode never times out early.
  const hb = setInterval(() => {
    dwell.heartbeat(Date.now(), visible());
    flush();
  }, 15000);

  // A click anywhere on a product page is a strong-interest signal (the engine's
  // click_detail coefficient). Once is enough to mark intent.
  document.addEventListener(
    "click",
    () => emit(C.clickDetail(itemId, ids, Date.now(), "external")),
    { capture: true, once: true }
  );

  const end = () => {
    clearInterval(hb);
    dwell.end(Date.now()); // emits impression_end with the total visible dwell_ms
    flush();
  };
  window.addEventListener("pagehide", end);
  window.addEventListener("beforeunload", end);

  // ---- helpers ----

  // Read the product id the user's own foreground page already received (Lane B:
  // no network request of our own). Shopify storefronts expose it on the analytics
  // meta; we only accept it on a /products/ path so listing/collection pages are
  // ignored. The id shape matches what A16a ingest stored, so dwell trains taste
  // on the real vector.
  function detectShopifyItemId() {
    if (!/\/products\//.test(location.pathname)) return null;
    const w = window;
    const pid =
      (w.ShopifyAnalytics && w.ShopifyAnalytics.meta && w.ShopifyAnalytics.meta.product && w.ShopifyAnalytics.meta.product.id) ||
      (w.meta && w.meta.product && w.meta.product.id) ||
      null;
    if (!pid) return null;
    const host = location.hostname.replace(/^www\./, "");
    return "shopify:" + host + ":" + pid;
  }

  async function sessionIds() {
    const uuid = () =>
      typeof crypto !== "undefined" && crypto.randomUUID
        ? crypto.randomUUID()
        : "id-" + Math.random().toString(16).slice(2) + "-" + Date.now();
    // Stable per-install device id (local), fresh per-tab session id.
    let deviceId;
    try {
      const o = await chrome.storage.local.get("modsearch.device");
      deviceId = o["modsearch.device"];
      if (!deviceId) {
        deviceId = uuid();
        await chrome.storage.local.set({ "modsearch.device": deviceId });
      }
    } catch (_e) {
      deviceId = uuid();
    }
    let sessionId = null;
    try {
      sessionId = sessionStorage.getItem("modsearch.session");
      if (!sessionId) {
        sessionId = uuid();
        sessionStorage.setItem("modsearch.session", sessionId);
      }
    } catch (_e) {
      sessionId = uuid();
    }
    return { uuid: uuid, sessionId: sessionId, deviceId: deviceId, userId: "local" };
  }
})();
