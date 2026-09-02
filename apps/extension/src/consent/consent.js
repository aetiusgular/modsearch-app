// A15 consent gate. NOTHING is captured until the user takes the explicit action
// on the first-run screen, and the content script checks captureAllowed() before
// attaching any listener. Stored in chrome.storage.local so it is shared across
// the extension and survives worker restarts. Plain JS so it loads as a content
// script (globalThis.__msConsent) and in the consent page alike.
// See MODSEARCH_PRD_v3.md Section 4.1: consent is a specific affirmative action,
// taken before any collection, disclosed in plain language, and the data stays
// local. The words "silently" and "telemetry" are deliberately absent here.
(function (root) {
  "use strict";

  const KEY = "modsearch.consent.v1";
  const DISCLOSURE =
    "With your permission, ModSearch observes the products you view to personalize " +
    "your recommendations. Everything is processed locally on your device. Your " +
    "browsing is never sold or shared, and you can turn it off at any time.";

  async function get() {
    try {
      const o = await chrome.storage.local.get(KEY);
      return o[KEY] || { capture: false, grantedAt: null };
    } catch (_e) {
      return { capture: false, grantedAt: null };
    }
  }

  async function setCapture(on, nowMs) {
    const state = {
      capture: !!on,
      grantedAt: on ? new Date(nowMs == null ? Date.now() : nowMs).toISOString() : null,
    };
    try {
      await chrome.storage.local.set({ [KEY]: state });
    } catch (_e) {
      /* storage unavailable: default stays off, which is the safe direction */
    }
    return state;
  }

  async function captureAllowed() {
    return (await get()).capture === true;
  }

  const api = { KEY, DISCLOSURE, get, setCapture, captureAllowed };
  if (typeof module !== "undefined" && module.exports) module.exports = api;
  root.__msConsent = api;
})(typeof globalThis !== "undefined" ? globalThis : this);
