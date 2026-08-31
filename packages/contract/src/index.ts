// @aurasearch/contract — the wire-contract entrypoint.
//
// The canonical schemas live in ../schema (JSON Schema, draft 2020-12) and mirror
// the engine's Pydantic models in aura-recs-engine/src/recs/schemas/. The engine's
// models are the parity reference (ADR-0001). Run `pnpm generate` to emit TypeScript
// interfaces into ./generated from those schemas.

import telemetryEventSchema from "../schema/telemetry-event.schema.json" with { type: "json" };
import listingChangeSchema from "../schema/listing-change.schema.json" with { type: "json" };

export const schemas = {
  telemetryEvent: telemetryEventSchema,
  listingChange: listingChangeSchema,
} as const;

export const EVENT_KINDS = [
  "impression_start", "impression_end", "heartbeat", "scroll_depth",
  "click_detail", "like", "unlike", "save", "unsave", "comment", "inquiry", "search",
] as const;
export type EventKind = (typeof EVENT_KINDS)[number];

export const LISTING_KINDS = ["created", "updated", "sold", "deleted"] as const;
export type ListingKind = (typeof LISTING_KINDS)[number];

// After `pnpm generate`, re-export the generated interfaces:
// export type { TelemetryEvent } from "./generated/telemetry-event.schema";
// export type { ListingChange } from "./generated/listing-change.schema";
