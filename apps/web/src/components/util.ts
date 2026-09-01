// Color helpers for tonal placeholder tiles derived from a garment's color.
function hexToRgb(hex: string): [number, number, number] {
  const h = hex.replace("#", "");
  return [parseInt(h.slice(0, 2), 16), parseInt(h.slice(2, 4), 16), parseInt(h.slice(4, 6), 16)];
}
function rgb(r: number, g: number, b: number) {
  const c = (n: number) => Math.max(0, Math.min(255, Math.round(n))).toString(16).padStart(2, "0");
  return `#${c(r)}${c(g)}${c(b)}`;
}
function mix(a: string, b: string, t: number) {
  const [r1, g1, b1] = hexToRgb(a);
  const [r2, g2, b2] = hexToRgb(b);
  return rgb(r1 + (r2 - r1) * t, g1 + (g2 - g1) * t, b1 + (b2 - b1) * t);
}
export function tile(colorHex: string): string {
  // a soft two-stop duotone of the garment color; slightly lifted highlight,
  // deeper shadow, angled like a draped fabric.
  const hi = mix(colorHex, "#ffffff", 0.22);
  const lo = mix(colorHex, "#000000", 0.28);
  return `linear-gradient(145deg, ${hi} 0%, ${colorHex} 46%, ${lo} 100%)`;
}
export function isLight(colorHex: string): boolean {
  const [r, g, b] = hexToRgb(colorHex);
  return 0.299 * r + 0.587 * g + 0.114 * b > 150;
}

export function money(n: number, currency = "USD"): string {
  return new Intl.NumberFormat("en-US", { style: "currency", currency, maximumFractionDigits: 0 }).format(n);
}

export function timeAgo(iso: string): string {
  const days = Math.floor((Date.now() - +new Date(iso)) / 86400000);
  if (days <= 0) return "today";
  if (days === 1) return "1 day ago";
  if (days < 30) return `${days} days ago`;
  const m = Math.floor(days / 30);
  return m === 1 ? "1 month ago" : `${m} months ago`;
}
