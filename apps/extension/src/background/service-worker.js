// AuraSearch extension background service worker (A14).
// Stateless relay between the full-page app (the SPA running as an extension page)
// and the native engine host. The MV3 worker is ephemeral, but the native port and
// in-flight messages keep it alive while a request is outstanding. No dwell/state
// lives here (that is the content script, A15).

const HOST = "com.aurasearch.engine";
let port = null;
const pending = new Map();

function ensurePort() {
  if (port) return port;
  port = chrome.runtime.connectNative(HOST);
  port.onMessage.addListener((msg) => {
    const cb = pending.get(msg.id);
    if (cb) { pending.delete(msg.id); cb(msg); }
  });
  port.onDisconnect.addListener(() => {
    const err = (chrome.runtime.lastError && chrome.runtime.lastError.message) || "engine disconnected";
    for (const [, cb] of pending) cb({ ok: false, error: err });
    pending.clear();
    port = null;
  });
  return port;
}

// App page -> SW -> native host, correlated by req.id.
chrome.runtime.onMessage.addListener((req, _sender, sendResponse) => {
  if (!req || typeof req.type !== "string") return;
  try {
    pending.set(req.id, sendResponse);
    ensurePort().postMessage(req);
  } catch (e) {
    sendResponse({ id: req.id, ok: false, error: String((e && e.message) || e) });
  }
  return true; // async response
});

// Toolbar click opens the full-page app in a tab (loads locally; no localhost).
chrome.action.onClicked.addListener(() => {
  chrome.tabs.create({ url: chrome.runtime.getURL("app/index.html") });
});
