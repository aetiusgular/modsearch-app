// MV3 background service worker. STATELESS RELAY ONLY.
// The MV3 worker is ephemeral (terminated after ~30s idle), so it must NOT hold dwell
// timers or long-lived state. Dwell/interaction measurement lives in the content script
// (src/content/dwell.ts). This worker only relays batched events to the local desktop app
// over native messaging, and keeps itself alive while that port is active.
// See AURASEARCH_PRD_v3.md Section 9.

export {};
// TODO: chrome.runtime.connectNative("dev.aurasearch.host"); relay batches; chrome.alarms flush.
