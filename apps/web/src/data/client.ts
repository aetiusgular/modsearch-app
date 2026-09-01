import { LISTINGS, dropInfo } from "./fixtures";
import type { Condition, FacetCounts, FeedQuery, Listing, MeasureKey } from "./types";

// The DataClient seam. Components depend only on this interface. Today it is
// MockDataClient over fixtures; A19 swaps in EngineDataClient that calls the
// Rust engine through the extension's native-messaging bridge. Same interface.

export interface DataClient {
  getFeed(query: FeedQuery, hidden: Set<string>): Promise<Listing[]>;
  getItem(id: string): Promise<Listing | undefined>;
  moreLikeThis(id: string, hidden: Set<string>): Promise<Listing[]>;
  getFacets(): Promise<FacetCounts>;
  getSaved(savedIds: Set<string>): Promise<Listing[]>;
  getDrops(hidden: Set<string>): Promise<Listing[]>;
  recordFeedback(id: string, kind: "like" | "save" | "hide"): Promise<void>;
}

const MEASURE_KEYS: MeasureKey[] = ["pit_to_pit", "shoulder", "length", "sleeve", "waist", "hip", "inseam", "rise", "thigh"];

function matches(l: Listing, q: FeedQuery): boolean {
  if (q.text) {
    const t = q.text.toLowerCase();
    const hay = `${l.brand} ${l.title} ${l.category} ${l.subcategory} ${l.color} ${l.era} ${l.aesthetic.join(" ")}`.toLowerCase();
    if (!hay.includes(t)) return false;
  }
  if (q.brands?.length && !q.brands.includes(l.brand)) return false;
  if (q.colors?.length && !q.colors.includes(l.color)) return false;
  if (q.categories?.length && !q.categories.includes(l.category)) return false;
  if (q.conditions?.length && !q.conditions.includes(l.condition)) return false;
  if (q.eras?.length && (!l.era || !q.eras.includes(l.era))) return false;
  if (q.sizes?.length && (!l.size || !q.sizes.includes(l.size))) return false;
  if (q.priceMin != null && l.price < q.priceMin) return false;
  if (q.priceMax != null && l.price > q.priceMax) return false;
  if (q.measures) {
    for (const k of MEASURE_KEYS) {
      const r = q.measures[k];
      if (!r) continue;
      const v = l.measurements.values[k];
      if (v == null) return false; // filtering on a measure the item lacks excludes it
      if (v < r[0] || v > r[1]) return false;
    }
  }
  return true;
}

function sortListings(rows: Listing[], q: FeedQuery, seedId?: string): Listing[] {
  const s = q.sort ?? "match";
  const arr = [...rows];
  if (s === "price_asc") arr.sort((a, b) => a.price - b.price);
  else if (s === "price_desc") arr.sort((a, b) => b.price - a.price);
  else if (s === "newest") arr.sort((a, b) => +new Date(b.listedAt) - +new Date(a.listedAt));
  else {
    // "match": mock the fused score. If seeded (more-like-this), boost same
    // category/color/brand so the relation is legible.
    const seed = seedId ? LISTINGS.find((x) => x.id === seedId) : undefined;
    const score = (l: Listing) => {
      let sc = l.matchScore;
      if (seed) {
        if (l.category === seed.category) sc += 0.25;
        if (l.color === seed.color) sc += 0.12;
        if (l.brand === seed.brand) sc += 0.18;
      }
      return sc;
    };
    arr.sort((a, b) => score(b) - score(a));
  }
  return arr;
}

export class MockDataClient implements DataClient {
  async getFeed(query: FeedQuery, hidden: Set<string>): Promise<Listing[]> {
    const rows = LISTINGS.filter((l) => !hidden.has(l.id) && matches(l, query));
    return sortListings(rows, query, query.moreLikeId);
  }
  async getItem(id: string): Promise<Listing | undefined> {
    return LISTINGS.find((l) => l.id === id);
  }
  async moreLikeThis(id: string, hidden: Set<string>): Promise<Listing[]> {
    const rows = LISTINGS.filter((l) => l.id !== id && !hidden.has(l.id));
    return sortListings(rows, { sort: "match", moreLikeId: id }, id).slice(0, 8);
  }
  async getSaved(savedIds: Set<string>): Promise<Listing[]> {
    return LISTINGS.filter((l) => savedIds.has(l.id));
  }
  async getDrops(hidden: Set<string>): Promise<Listing[]> {
    return LISTINGS.filter((l) => !hidden.has(l.id) && dropInfo(l).dropped)
      .sort((a, b) => dropInfo(a).pct - dropInfo(b).pct);
  }
  async recordFeedback(): Promise<void> {
    // no-op in mock; EngineDataClient will send a TelemetryEvent to the engine.
  }
  async getFacets(): Promise<FacetCounts> {
    const count = <T,>(sel: (l: Listing) => T | undefined) => {
      const m = new Map<T, number>();
      LISTINGS.forEach((l) => { const v = sel(l); if (v != null) m.set(v, (m.get(v) ?? 0) + 1); });
      return [...m.entries()].sort((a, b) => b[1] - a[1]);
    };
    const colorsMap = new Map<string, [string, number]>();
    LISTINGS.forEach((l) => {
      const e = colorsMap.get(l.color) ?? [l.colorHex, 0];
      colorsMap.set(l.color, [l.colorHex, e[1] + 1]);
    });
    const prices = LISTINGS.map((l) => l.price);
    const measureRanges: Partial<Record<MeasureKey, [number, number]>> = {};
    MEASURE_KEYS.forEach((k) => {
      const vals = LISTINGS.map((l) => l.measurements.values[k]).filter((v): v is number => v != null);
      if (vals.length) measureRanges[k] = [Math.floor(Math.min(...vals)), Math.ceil(Math.max(...vals))];
    });
    return {
      brands: count((l) => l.brand) as [string, number][],
      colors: [...colorsMap.entries()].map(([name, [hex, n]]) => [name, hex, n] as [string, string, number]).sort((a, b) => b[2] - a[2]),
      categories: count((l) => l.category) as [string, number][],
      conditions: count((l) => l.condition) as [Condition, number][],
      eras: count((l) => l.era) as [string, number][],
      sizes: count((l) => l.size) as [string, number][],
      priceRange: [Math.floor(Math.min(...prices)), Math.ceil(Math.max(...prices))],
      measureRanges,
    };
  }
}

export const client: DataClient = new MockDataClient();
