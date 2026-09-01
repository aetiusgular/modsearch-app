// App-facing view models. These are the shapes the UI renders. They are a
// superset/projection of @aurasearch/contract's ListingChange payload plus the
// recs signals the engine attaches (matchScore, matchReasons). The DataClient
// boundary maps contract <-> view models, so swapping MockDataClient for
// EngineDataClient (A19) never touches the components.

export type Condition = "new" | "like_new" | "excellent" | "good" | "fair" | "poor";

export const CONDITION_LABEL: Record<Condition, string> = {
  new: "New",
  like_new: "Like new",
  excellent: "Excellent",
  good: "Good",
  fair: "Fair",
  poor: "Poor",
};

export type MeasureKey =
  | "pit_to_pit" | "shoulder" | "length" | "sleeve"
  | "waist" | "hip" | "inseam" | "rise" | "thigh";

export const MEASURE_LABEL: Record<MeasureKey, string> = {
  pit_to_pit: "Pit to pit",
  shoulder: "Shoulder",
  length: "Length",
  sleeve: "Sleeve",
  waist: "Waist",
  hip: "Hip",
  inseam: "Inseam",
  rise: "Rise",
  thigh: "Thigh",
};

export interface Measurements {
  unit: "cm" | "in";
  values: Partial<Record<MeasureKey, number>>;
  source_field: "structured" | "parsed_text" | "ocr_photo" | "user_entered";
}

export interface PricePoint {
  t: string; // ISO date
  price: number;
}

export interface Listing {
  id: string;
  brand: string;
  title: string;
  category: string;
  subcategory?: string;
  era?: string;
  color: string;
  colorHex: string;
  size?: string;
  condition: Condition;
  price: number;
  currency: string;
  source: string;
  sourceKind: "boutique" | "marketplace";
  listingUrl: string;
  measurements: Measurements;
  aesthetic: string[];
  imageUrl?: string;
  listedAt: string;
  priceHistory: PricePoint[];
  matchScore: number;
  matchReasons: string[];
}

export type SortKey = "match" | "price_asc" | "price_desc" | "newest";

export interface FeedQuery {
  text?: string;
  measures?: Partial<Record<MeasureKey, [number, number]>>;
  sizes?: string[];
  colors?: string[];
  brands?: string[];
  priceMin?: number;
  priceMax?: number;
  conditions?: Condition[];
  categories?: string[];
  eras?: string[];
  moreLikeId?: string;
  sort?: SortKey;
}

export type FeedbackKind = "like" | "save" | "hide";

export interface FacetCounts {
  brands: [string, number][];
  colors: [string, string, number][]; // name, hex, count
  categories: [string, number][];
  conditions: [Condition, number][];
  eras: [string, number][];
  sizes: [string, number][];
  priceRange: [number, number];
  measureRanges: Partial<Record<MeasureKey, [number, number]>>;
}
