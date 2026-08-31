# @aurasearch/extension

The AuraSearch browser extension. One TypeScript core, three builds (ADR-0001, PRD Section 9):

- Chromium (Chrome + Edge): MV3 service worker. It is ephemeral, so dwell timing lives in the
  content script, not the worker. The worker is a stateless relay to the local desktop app over
  native messaging.
- Firefox: MV3 with non-persistent event pages (not service workers) and a different native-
  messaging host-manifest format. Second build, shared core.
- Safari: a separate Xcode app project, App Store distribution. Native messaging routes to a
  SafariWebExtensionHandler bundled in the containing macOS app, not a standalone daemon. Its own
  workstream, not built here yet.

## Consent, not silence

Nothing is captured before the first-run consent gate (`src/consent/consent.ts`). Interaction
logging is the disclosed single purpose, local-only, never sold or shared. This is what keeps the
extension approvable under Chrome Web Store 2026 policy. See PRD Section 4.1.

## Status

Stub. `src/` holds documented placeholders for the background relay, the content-script dwell
tracker, and the consent gate. Manifests for Chromium and Firefox are present. No bundler wired yet.
