# @modsearch/extension

The ModSearch browser extension. One TypeScript core, three builds (ADR-0001, PRD Section 9):

- Chromium (Chrome + Edge): MV3 service worker. It is ephemeral, so dwell timing lives in the
  content script, not the worker. The worker is a stateless relay to the local desktop app over
  native messaging.
- Firefox: MV3 with non-persistent event pages (not service workers) and a different native-
  messaging host-manifest format. Second build, shared core.
- Safari: a separate Xcode app project, App Store distribution. Native messaging routes to a
  SafariWebExtensionHandler bundled in the containing macOS app, not a standalone daemon. Its own
  workstream, not built here yet.

## Consent, not silence

Nothing is captured before the first-run consent gate (`src/consent/consent.js` +
`consent.html`, shown on install). Opt-in is an explicit action, interaction observation is the
disclosed single purpose, local-only, never sold or shared. This is what keeps the extension
approvable under Chrome Web Store 2026 policy. See PRD Section 4.1.

## A15: capture (built)

Plain-JS content scripts, no bundler. Loaded in manifest order so they share one isolated world:

- `src/content/capture-core.js` — pure dwell/episode logic and the contract event builders. Unit
  tested in node: `pnpm -C apps/extension test` (or `node src/content/capture-core.test.js`).
- `src/consent/consent.js` — the opt-in flag in `chrome.storage.local` and the disclosure copy.
- `src/content/dwell.js` — the per-tab content script. Returns immediately unless consent is granted;
  otherwise, on a Shopify `/products/` page it reads the product id the page already has, forms the
  `shopify:<domain>:<id>` catalog id, measures foreground dwell via visibilitychange + 15s
  heartbeats + pagehide, and batches `recordEvents` to the engine through the worker relay. The id
  matches what A16a ingested, so dwell trains taste on the real image vector.

`content_scripts` currently match the one ingested boutique (`nomanwalksalone.com`); the sanctioned
host list is registry-driven later (A23). Note: the contract annotates `item_id` as a uuid, which
predates the string catalog ids (`shopify:...`, `it_...`) the engine actually uses; the engine
treats it as an opaque string, so it is a stale annotation to relax, not a blocker.

### Load and try it

1. Run the engine as the native host against the ingested catalog: install the host
   (`host/install-host.sh`) and make sure it launches the engine with
   `MOD_ENGINE_DB=$HOME/.modsearch/boutiques.db`.
2. `bash build.sh` to stage the web app, then load `apps/extension` unpacked (chrome://extensions,
   Developer mode, Load unpacked). The consent screen opens on install; click Enable.
3. Open a `nomanwalksalone.com` product page, dwell, then leave it. The content script emits
   impression_start/heartbeat/impression_end + a click to the engine; the engine folds them into an
   episode and updates taste on that item's real vector. Re-run `feed-html` to see the feed shift.
