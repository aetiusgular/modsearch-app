import { CONDITION_LABEL, type Listing } from "../data/types";
import { dropInfo } from "../data/fixtures";
import { Icon } from "./Icons";
import { money, tile, isLight, timeAgo } from "./util";
import { useApp } from "../state/store";

function keyMeasure(l: Listing): string {
  const v = l.measurements.values;
  const u = l.measurements.unit;
  if (v.pit_to_pit) return `P2P ${v.pit_to_pit}${u}${v.length ? ` · L ${v.length}${u}` : ""}`;
  if (v.waist) return `W ${v.waist}${u}${v.inseam ? ` · IN ${v.inseam}${u}` : ""}`;
  return l.size ? `Size ${l.size}` : "";
}

export function ItemCard({ l }: { l: Listing }) {
  const { saved, liked, toggleFeedback, openDetail } = useApp();
  const isSaved = saved.has(l.id);
  const isLiked = liked.has(l.id);
  const drop = dropInfo(l);
  const light = isLight(l.colorHex);
  const onImg = light ? "rgba(20,22,26,.62)" : "rgba(255,255,255,.72)";

  const act = (e: React.MouseEvent, kind: "like" | "save" | "hide") => {
    e.stopPropagation();
    toggleFeedback(l.id, kind);
  };

  return (
    <article
      className="card rise"
      style={{ overflow: "hidden", cursor: "pointer", display: "flex", flexDirection: "column" }}
      onClick={() => openDetail(l.id)}
      onKeyDown={(e) => { if (e.key === "Enter") openDetail(l.id); }}
      tabIndex={0}
      role="button"
      aria-label={`${l.brand} ${l.subcategory}, ${money(l.price, l.currency)}`}
    >
      <div style={{ position: "relative", aspectRatio: "3 / 4", background: l.imageUrl ? "var(--surface-2)" : tile(l.colorHex) }}>
        {l.imageUrl && <img src={l.imageUrl} alt={l.title} style={{ width: "100%", height: "100%", objectFit: "cover" }} />}
        {!l.imageUrl && (
          <span style={{
            position: "absolute", inset: 0, display: "grid", placeItems: "center",
            fontFamily: "var(--font-mono)", fontSize: 10, letterSpacing: ".18em",
            textTransform: "uppercase", color: onImg,
          }}>{l.category}</span>
        )}

        {/* top badges */}
        <div style={{ position: "absolute", top: 8, left: 8, right: 8, display: "flex", justifyContent: "space-between", gap: 6 }}>
          <span className="pill" style={{ background: "var(--surface)", opacity: 0.96 }}>
            <span style={{ width: 5, height: 5, borderRadius: 9, background: l.sourceKind === "boutique" ? "var(--accent)" : "var(--muted)" }} />
            {l.source.replace("shopify:", "")}
          </span>
          <span className="pill" style={{ background: "var(--surface)", opacity: 0.96 }}>{CONDITION_LABEL[l.condition]}</span>
        </div>

        {/* drop flag */}
        {drop.dropped && (
          <span className="pill pill-good" style={{ position: "absolute", left: 8, bottom: 8 }}>▼ {Math.abs(drop.pct)}%</span>
        )}

        {/* hover actions */}
        <div className="card-actions" style={{
          position: "absolute", right: 8, bottom: 8, display: "flex", gap: 6,
        }}>
          <button className="icon-act" aria-label={isLiked ? "Unlike" : "Like"} aria-pressed={isLiked}
            onClick={(e) => act(e, "like")} style={{ color: isLiked ? "var(--alert)" : "var(--ink)" }}>
            <Icon.Heart size={16} fill={isLiked} />
          </button>
          <button className="icon-act" aria-label={isSaved ? "Remove from saved" : "Save"} aria-pressed={isSaved}
            onClick={(e) => act(e, "save")} style={{ color: isSaved ? "var(--accent)" : "var(--ink)" }}>
            <Icon.Bookmark size={16} fill={isSaved} />
          </button>
          <button className="icon-act" aria-label="Not for me" onClick={(e) => act(e, "hide")}>
            <Icon.Hide size={16} />
          </button>
        </div>
      </div>

      <div style={{ padding: "10px 11px 12px", display: "flex", flexDirection: "column", gap: 6, flex: 1 }}>
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "baseline", gap: 8 }}>
          <span className="font-display" style={{ fontWeight: 700, fontSize: 13.5, letterSpacing: "-.01em", lineHeight: 1.15 }}>{l.brand}</span>
          <span className="font-mono tnum" style={{ fontSize: 13, fontWeight: 600 }}>{money(l.price, l.currency)}</span>
        </div>
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "baseline", gap: 8, color: "var(--muted)", fontSize: 12 }}>
          <span style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{l.era} {l.subcategory}</span>
          <span className="font-mono" style={{ color: "var(--faint)", fontSize: 11, whiteSpace: "nowrap" }}>{keyMeasure(l)}</span>
        </div>
        {l.matchReasons[0] && (
          <div style={{ marginTop: 1 }}>
            <span className="pill pill-accent"><Icon.Bolt size={11} /> {l.matchReasons[0]}</span>
          </div>
        )}
      </div>
    </article>
  );
}
