import { TopBar } from "../components/AppShell";
import { Grid } from "../components/Grid";
import { Sparkline } from "../components/Sparkline";
import { Icon } from "../components/Icons";
import { useDrops, useSaved } from "../data/useListings";
import { dropInfo } from "../data/fixtures";
import { useApp } from "../state/store";
import { money, tile } from "../components/util";
import type { Listing } from "../data/types";

export function Saved() {
  const rows = useSaved();
  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100%" }}>
      <TopBar title="Saved" subtitle="Your watchlist. We track the price of everything here." showSort={false} />
      <main className="scroll-y" style={{ flex: 1, padding: "20px 22px 48px" }}>
        {rows && rows.length === 0 ? (
          <div style={{ color: "var(--muted)", padding: "56px 0", textAlign: "center" }}>
            <p className="font-display" style={{ fontSize: 18, fontWeight: 700, color: "var(--ink)", margin: "0 0 6px" }}>Nothing saved yet</p>
            <p style={{ margin: 0, fontSize: 13 }}>Tap the bookmark on any piece to watch it and track its price.</p>
          </div>
        ) : <Grid rows={rows} />}
      </main>
    </div>
  );
}

function DropRow({ l }: { l: Listing }) {
  const { openDetail } = useApp();
  const d = dropInfo(l);
  return (
    <button onClick={() => openDetail(l.id)} className="card"
      style={{ display: "flex", alignItems: "center", gap: 14, padding: 12, cursor: "pointer", textAlign: "left", width: "100%" }}>
      <span style={{ width: 52, height: 66, borderRadius: 7, background: tile(l.colorHex), flexShrink: 0 }} />
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{ display: "flex", gap: 8, alignItems: "baseline" }}>
          <span className="font-display" style={{ fontWeight: 700, fontSize: 14 }}>{l.brand}</span>
          <span style={{ color: "var(--muted)", fontSize: 12.5, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{l.era} {l.subcategory}</span>
        </div>
        <div style={{ display: "flex", alignItems: "center", gap: 9, marginTop: 4 }}>
          <span className="font-mono tnum" style={{ fontSize: 12.5, color: "var(--faint)", textDecoration: "line-through" }}>{money(d.from)}</span>
          <span className="font-mono tnum" style={{ fontSize: 15, fontWeight: 600 }}>{money(d.to)}</span>
          <span className="pill pill-good">▼ {Math.abs(d.pct)}%</span>
        </div>
      </div>
      <Sparkline points={l.priceHistory} good w={110} h={38} />
      <span className="pill" style={{ flexShrink: 0 }}>{l.source.replace("shopify:", "")}</span>
    </button>
  );
}

export function Drops() {
  const rows = useDrops();
  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100%" }}>
      <TopBar title="Drops" subtitle="Price cuts and restocks on pieces matched to your taste. Also sent to your email." showSort={false} />
      <main className="scroll-y" style={{ flex: 1, padding: "20px 22px 48px" }}>
        {rows && rows.length === 0 ? (
          <div style={{ color: "var(--muted)", padding: "56px 0", textAlign: "center" }}>
            <p className="font-display" style={{ fontSize: 18, fontWeight: 700, color: "var(--ink)", margin: "0 0 6px" }}>No drops right now</p>
            <p style={{ margin: 0, fontSize: 13 }}>When something you'd like falls in price, it shows up here.</p>
          </div>
        ) : (
          <div style={{ display: "flex", flexDirection: "column", gap: 10, maxWidth: 760 }}>
            {rows?.map((l) => <DropRow key={l.id} l={l} />)}
          </div>
        )}
      </main>
    </div>
  );
}
