import type { Condition, Listing, MeasureKey, Measurements, PricePoint } from "./types";

// Deterministic synthetic catalog. Seeded RNG so the feed is stable across
// reloads (matches the engine's "cursor pages frozen" behavior). Placeholder
// images are generated tonally from each item's color in ItemCard; no external
// assets, so this renders self-contained.

function mulberry32(seed: number) {
  return function () {
    seed |= 0;
    seed = (seed + 0x6d2b79f5) | 0;
    let t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}
const rand = mulberry32(20260901);
const pick = <T,>(a: T[]): T => a[Math.floor(rand() * a.length)];
const pickN = <T,>(a: T[], n: number): T[] => {
  const c = [...a];
  const out: T[] = [];
  for (let i = 0; i < n && c.length; i++) out.push(c.splice(Math.floor(rand() * c.length), 1)[0]);
  return out;
};
const between = (lo: number, hi: number) => lo + rand() * (hi - lo);
const round = (n: number, s = 1) => Math.round(n / s) * s;

const BRANDS = [
  "Maison Margiela", "Rick Owens", "Kapital", "Stone Island", "Yohji Yamamoto",
  "Undercover", "Number (N)ine", "Raf Simons", "Comme des Garçons", "Issey Miyake",
  "Junya Watanabe", "visvim", "Needles", "Our Legacy", "Lemaire", "Auralee",
  "Stüssy", "Levi's", "Carhartt WIP", "Nike", "New Balance", "Salomon",
  "Arc'teryx", "Acronym", "Engineered Garments", "Bode", "Cav Empt", "Sacai",
];

const COLORS: [string, string][] = [
  ["Black", "#1c1c1e"], ["Charcoal", "#3a3d42"], ["Slate", "#5b6570"],
  ["Ecru", "#d9d2c2"], ["Sand", "#c9b998"], ["Olive", "#5c5f3c"],
  ["Forest", "#2f4636"], ["Navy", "#26314a"], ["Indigo", "#3a4d74"],
  ["Rust", "#8a4a34"], ["Oxblood", "#5e2b2f"], ["Cream", "#eae3d2"],
  ["Grey Melange", "#8a8d90"], ["Brown", "#5a4433"], ["Burgundy", "#4e2230"],
  ["Washed Blue", "#7e97ac"],
];

type CatSpec = {
  name: string;
  subs: string[];
  measures: Partial<Record<MeasureKey, [number, number]>>;
  sizes: string[];
  eraBias?: string[];
  priceBias: [number, number];
};
const CATS: CatSpec[] = [
  { name: "Outerwear", subs: ["Field jacket", "Parka", "Trench", "Bomber", "Chore coat", "Leather jacket"],
    measures: { pit_to_pit: [54, 66], shoulder: [44, 54], length: [68, 86], sleeve: [62, 70] },
    sizes: ["44", "46", "48", "50", "S", "M", "L"], priceBias: [180, 1400] },
  { name: "Knitwear", subs: ["Crewneck", "Cardigan", "Mohair sweater", "Zip knit"],
    measures: { pit_to_pit: [50, 62], shoulder: [42, 52], length: [62, 76], sleeve: [60, 70] },
    sizes: ["S", "M", "L", "XL"], priceBias: [90, 520] },
  { name: "Tops", subs: ["Tee", "Overshirt", "Oxford shirt", "Rugby"],
    measures: { pit_to_pit: [48, 62], shoulder: [42, 52], length: [66, 78], sleeve: [20, 66] },
    sizes: ["S", "M", "L", "XL"], priceBias: [40, 320] },
  { name: "Bottoms", subs: ["Denim", "Trouser", "Cargo", "Wide pant"],
    measures: { waist: [72, 96], inseam: [66, 82], rise: [24, 34], thigh: [28, 40] },
    sizes: ["28", "30", "32", "34", "36"], priceBias: [80, 620] },
  { name: "Footwear", subs: ["Trainer", "Derby", "Boot", "GAT"],
    measures: {}, sizes: ["7", "8", "9", "10", "11", "12"], priceBias: [90, 780] },
];

const ERAS = ["1980s", "1990s", "Y2K", "2010s", "Contemporary"];
const AESTHETICS = ["Techwear", "Americana", "Avant-Garde", "Workwear", "Minimalism", "Gorpcore", "Archive", "Ivy"];
const SOURCES: [string, "boutique" | "marketplace"][] = [
  ["grailed", "marketplace"], ["ebay", "marketplace"], ["vestiaire", "marketplace"],
  ["depop", "marketplace"], ["shopify:no-man-walks-alone", "boutique"],
  ["shopify:lost-found", "boutique"], ["shopify:corlectic", "boutique"], ["agora", "marketplace"],
];
const CONDITIONS: Condition[] = ["new", "like_new", "excellent", "good", "fair"];

const DAY = 86400000;
function history(base: number, drops: boolean): PricePoint[] {
  const pts: PricePoint[] = [];
  let p = base * between(1.05, 1.35);
  const now = Date.now();
  const n = 5 + Math.floor(rand() * 4);
  for (let i = n; i >= 0; i--) {
    if (drops && rand() < 0.5) p = Math.max(base, p * between(0.82, 0.97));
    pts.push({ t: new Date(now - i * 9 * DAY).toISOString().slice(0, 10), price: round(p) });
  }
  pts[pts.length - 1] = { t: pts[pts.length - 1].t, price: base };
  return pts;
}

function makeMeasures(cat: CatSpec): Measurements {
  const values: Partial<Record<MeasureKey, number>> = {};
  (Object.entries(cat.measures) as [MeasureKey, [number, number]][]).forEach(([k, [lo, hi]]) => {
    values[k] = round(between(lo, hi), 0.5);
  });
  const sf = rand() < 0.55 ? "structured" : rand() < 0.7 ? "parsed_text" : rand() < 0.85 ? "ocr_photo" : "user_entered";
  return { unit: "cm", values, source_field: sf as Measurements["source_field"] };
}

function titleFor(brand: string, sub: string, era?: string): string {
  const bits = [era && rand() < 0.5 ? era : "", brand, sub].filter(Boolean);
  return bits.join(" ");
}

export const LISTINGS: Listing[] = Array.from({ length: 96 }, (_, i) => {
  const cat = pick(CATS);
  const sub = pick(cat.subs);
  const [color, colorHex] = pick(COLORS);
  const [source, sourceKind] = pick(SOURCES);
  const era = pick(ERAS);
  const base = round(between(cat.priceBias[0], cat.priceBias[1]), 5);
  const dropsFlag = rand() < 0.4;
  const hist = history(base, dropsFlag);
  const brand = pick(BRANDS);
  const cond = pick(CONDITIONS);
  const matchScore = Math.min(0.99, Math.max(0.32, 0.5 + (rand() - 0.4) * 0.9));
  const reasons = pickN(
    [`Similar to saved ${pick(CATS).name.toLowerCase()}`, `${color} you tend to like`,
     `${brand} in your history`, `Fits your measurements`, `${pick(AESTHETICS)} match`],
    2,
  );
  return {
    id: `it_${(i + 1).toString().padStart(3, "0")}`,
    brand,
    title: titleFor(brand, sub, era),
    category: cat.name,
    subcategory: sub,
    era,
    color,
    colorHex,
    size: pick(cat.sizes),
    condition: cond,
    price: base,
    currency: "USD",
    source,
    sourceKind,
    listingUrl: "https://example.com/listing/" + (i + 1),
    measurements: makeMeasures(cat),
    aesthetic: pickN(AESTHETICS, 1 + Math.floor(rand() * 2)),
    listedAt: new Date(Date.now() - Math.floor(rand() * 40) * DAY).toISOString(),
    priceHistory: hist,
    matchScore,
    matchReasons: reasons,
  };
});

export function dropInfo(l: Listing): { dropped: boolean; from: number; to: number; pct: number } {
  const first = l.priceHistory[0]?.price ?? l.price;
  const last = l.price;
  const pct = first > 0 ? Math.round(((last - first) / first) * 100) : 0;
  return { dropped: pct <= -3, from: first, to: last, pct };
}
