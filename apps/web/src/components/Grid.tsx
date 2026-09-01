import type { Listing } from "../data/types";
import { ItemCard } from "./ItemCard";

export function Grid({ rows }: { rows: Listing[] | null }) {
  if (rows === null) {
    return (
      <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fill, minmax(200px, 1fr))", gap: 16 }}>
        {Array.from({ length: 8 }).map((_, i) => (
          <div key={i} className="card" style={{ aspectRatio: "3 / 4.5", background: "var(--surface-2)", opacity: 0.5 }} />
        ))}
      </div>
    );
  }
  if (rows.length === 0) {
    return (
      <div style={{ display: "grid", placeItems: "center", padding: "72px 0", textAlign: "center", color: "var(--muted)" }}>
        <div>
          <p className="font-display" style={{ fontSize: 18, fontWeight: 700, color: "var(--ink)", margin: "0 0 6px" }}>Nothing matches yet</p>
          <p style={{ margin: 0, fontSize: 13 }}>Loosen a filter, or add a store from the extension to widen the net.</p>
        </div>
      </div>
    );
  }
  return (
    <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fill, minmax(200px, 1fr))", gap: 16 }}>
      {rows.map((l) => <ItemCard key={l.id} l={l} />)}
    </div>
  );
}
