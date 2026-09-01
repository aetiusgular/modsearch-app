// Per-tab content script. Measures dwell and interaction on pages the user actively views.
// Lives as long as the page, so it (not the service worker) owns dwell timing, using
// visibilitychange + IntersectionObserver + heartbeats. Emits @modsearch/contract
// TelemetryEvents to the background relay. Foreground-only: no background/paginated queries.
// See MODSEARCH_PRD_v3.md Sections 4 and 9.

export {};
// TODO: IntersectionObserver for impression_start/end; visibilitychange heartbeats; dwell_ms.
