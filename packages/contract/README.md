# @aurasearch/contract

The single source of truth for AuraSearch's wire contract: the telemetry events the extension sends, and the listing-change events the catalog produces. Every consumer agrees on these shapes.

Consumers:
- the browser extension (TypeScript) emits `TelemetryEvent`s
- the desktop client (Rust) receives and validates them
- the engine (`aura-recs-engine`, Python now, Rust port later) scores them
- the backend (`aurasearch-server`) ingests and archives them

## Canonical form

The schemas live in `schema/` as JSON Schema (draft 2020-12), language-neutral on purpose:

- `telemetry-event.schema.json` — the 12-kind event union (`type` discriminator)
- `listing-change.schema.json` — the 4-kind listing-change union (`kind` discriminator)

These mirror the engine's Pydantic models in `aura-recs-engine/src/recs/schemas/` exactly. The engine's models are the parity reference. If you change a shape, change it here and in the engine in the same pull request, and bump the package version. This is the discipline ADR-0001 relies on to stop the contract drifting across four consumers in three languages.

## Generated types

- TypeScript: `pnpm generate` runs `json-schema-to-typescript` over `schema/` into `src/generated/`. `src/index.ts` re-exports them plus the raw schemas.
- Rust: generate with `typify` from the same JSON Schema in the desktop client's build (planned; see `apps/desktop`).
- Python: the engine already defines these as Pydantic. CI validates the engine's models against these JSON Schemas so the two cannot diverge silently (planned).

## Field-name boundary (D-06)

The wire uses `dwell_ms` and `max_viewport_pct`. The engine's executable reference uses `dwell_time_ms` and `viewport_ratio`. The engine's aggregator is the mapping boundary. The wire (this package) always uses the `dwell_ms` / `max_viewport_pct` names.

## Planned v1.1 (AuraSearch extensions, not yet in the schema)

AuraSearch is universal search across marketplaces, so `ListingPayload` will gain, as an additive change:
- `source` — the marketplace or boutique the listing came from (ebay, shopify:<store>, agora, ...)
- `listing_url` — the canonical URL to the item
- `external_id` — the source's own id
- `checked_at` — when availability was last verified, for the staleness handling the PRD requires (secondhand listings are quantity-one and vanish fast)
- `condition_tier` — a normalized condition scale for cross-marketplace comparison

And `TelemetryEvent` will gain the extended coefficients the engine already supports behind a flag: `offer`, `purchase`, `hide`. These are held out of v1 to keep parity with the engine's golden reference until the port lands. See `AURASEARCH_PRD_v3.md` sections 3 and 4.
