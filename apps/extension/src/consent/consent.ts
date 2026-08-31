// First-run consent gate. NOTHING is captured before the user takes an explicit action here.
// Required by Chrome Web Store 2026 policy: interaction logging is the disclosed single purpose,
// consent is a specific affirmative action taken BEFORE collection, and data stays local.
// The words "silently" and "telemetry" are deliberately absent from user-facing copy.
// See AURASEARCH_PRD_v3.md Section 4.1.

export {};
// TODO: show disclosure; require explicit opt-in; gate all listeners on the stored consent flag.
