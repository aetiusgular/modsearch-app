# @modsearch/contract

The single source of truth for ModSearch's wire contract: the telemetry events the extension sends, and the listing-change events the catalog produces. Every consumer agrees on these shapes.

Consumers:
- the browser extension (TypeScript) emits `TelemetryEvent`s
- the desktop client (Rust) receives and validates them
- the engine (`mod-recs-engine`, Python now, Rust port later) scores them
- the backend (`modsearch-server`) ingests and archives them

## Canonical form

The schemas live in `schema/` as JSON Schema (draft 2020-12), language-neutral on purpose:

- `telemetry-event.schema.json` — the 12-kind event union (`type` discriminator)
- `listing-change.schema.json` — the 4-kind listing-change union (`kind` discriminator)

These mirror the engine's Pydantic models in `mod-recs-engine/src/recs/schemas/` exactly. The engine's models are the parity reference. If you change a shape, change it here and in the engine in the same pull request, and bump the package version. This is the discipline ADR-0001 relies on to stop the contract drifting across four consumers in three languages.

## Generated types

- TypeScript: `pnpm generate` runs `json-schema-to-typescript` over `schema/` into `src/generated/`. `src/index.ts` re-exports them plus the raw schemas.
- Rust: generate with `typify` from the same JSON Schema in the desktop client's build (planned; see `apps/desktop`).
- Python: the engine already defines these as Pydantic. CI validates the engine's models against these JSON Schemas so the two cannot diverge silently (planned).

## Field-name boundary (D-06)

The wire uses `dwell_ms` and `max_viewport_pct`. The engine's executable reference uses `dwell_time_ms` and `viewport_ratio`. The engine's aggregator is the mapping boundary. The wire (this package) always uses the `dwell_ms` / `max_viewport_pct` names.

## ModSearch extensions in the listing schema (added)

`listing-change.schema.json` now carries ModSearch's cross-marketplace fields on `ListingPayload`, marked with an MODSEARCH EXTENSION note in each field description:
- `source` — the marketplace or boutique the listing came from (ebay, shopify:<store>, grailed, agora, ...)
- `listing_url`, `external_id`, `currency`
- `color` — normalized primary color, for the color filter
- `condition_tier` — a normalized condition scale for cross-marketplace comparison
- `checked_at` — when availability/price was last verified, for staleness handling (secondhand listings are quantity-one and vanish fast)
- `measurements` — a normalized garment-measurement object (pit_to_pit, shoulder, length, sleeve, waist, hip, inseam, rise, thigh), the key differentiator for filtering. `source_field` flags whether each came from structured data, parsed text, OCR of a measurement photo, or the user.

The forked engine (`mod-recs-engine`) must add matching optional fields when these are wired; they are additive and do not change the engine's existing golden reference.

## Planned v1.1 (telemetry, not yet in the schema)

`TelemetryEvent` will gain the extended coefficients the engine already supports behind a flag: `offer`, `purchase`, `hide`. These are held out of v1 to keep parity with the engine's golden reference until the port lands. See `MODSEARCH_PRD_v3.md` sections 3 and 4.
