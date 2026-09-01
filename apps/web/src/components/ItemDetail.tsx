import { useEffect, useState } from "react";
import { client } from "../data/client";
import { dropInfo } from "../data/fixtures";
import { CONDITION_LABEL, MEASURE_LABEL, type Listing, type MeasureKey } from "../data/types";
import { useApp } from "../state/store";
import { Icon } from "./Icons";
import { Sparkline } from "./Sparkline";
import { ItemCard } from "./ItemCard";
import { money, tile, isLight, timeAgo } from "./util";

const MEASURE_ORDER: MeasureKey[] = ["pit_to_pit", "shoulder", "length", "sleeve", "waist", "hip", "inseam", "rise", "thigh"];

const SRC_NOTE: Record<Listing["measurements"]["source_field"], string> = {
  structured: "from the listing's fields",
  parsed_text: "parsed from the description",
  ocr_photo: "read from a measurement photo",
  user_entered: "entered by a person",
};

export function ItemDetail() {
  const { detailId, closeDetail, saved, liked, toggleFeedback, hidden, openDetail } = useApp();
  const [item, setItem] = useState<Listing | null>(null);
  const [more, setMore] = useState<Listing[]>([]);

  useEffect(() => {
    if (!detailId) { setItem(null); return; }
    client.getItem(detailId).then((i) => setItem(i ?? null));
    client.moreLikeThis(detailId, hidden).then(setMore);
    const onKey = (e: KeyboardEvent) => { if (e.key === "Escape") closeDetail(); };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [detailId]);

  if (!detailId || !item) return null;
  const l = item;
  const drop = dropInfo(l);
  const light = isLight(l.colorHex);
  const onImg = light ? "rgba(20,22,26,.5)" : "rgba(255,255,255,.6)";
  const isSaved = saved.has(l.id);
  const isLiked = liked.has(l.id);
  const measures = MEASURE_ORDER.filter((k) => l.measurements.values[k] != null);

  return (
    <div role="dialog" aria-modal="true" aria-label={`${l.brand} ${l.subcategory}`}
      onClick={closeDetail}
      style={{ position: "fixed", inset: 0, zIndex: 40, background: "rgba(10,11,13,.5)", backdropFilter: "blur(3px)", display: "grid", placeItems: "center", padding: 20 }}>
      <div className="card rise scroll-y" onClick={(e) => e.stopPropagation()}
        style={{ width: "min(940px, 100%)", maxHeight: "90vh", boxShadow: "var(--shadow)", overflow: "auto" }}>
        <div style={{ display: "grid", gridTemplateColumns: "minmax(0, 1.05fr) minmax(0, 1fr)" }}>
          {/* gallery */}
          <div style={{ position: "relative", minHeight: 420, background: tile(l.colorHex), display: "flex", flexDirection: "column", justifyContent: "space-between", padding: 14 }}>
            <div style={{ display: "flex", justifyContent: "space-between" }}>
              <span className="pill" style={{ opacity: 0.96 }}>
                <span style={{ width: 5, height: 5, borderRadius: 9, background: l.sourceKind === "boutique" ? "var(--accent)" : "var(--muted)" }} />
                {l.source.replace("shopify:", "")}
              </span>
              <span className="pill" style={{ opacity: 0.96 }}>{CONDITION_LABEL[l.condition]}</span>
            </div>
            <span style={{ position: "absolute", inset: 0, display: "grid", placeItems: "center", fontFamily: "var(--font-mono)", fontSize: 12, letterSpacing: ".2em", textTransform: "uppercase", color: onImg }}>{l.category}</span>
            <div style={{ display: "flex", gap: 8 }}>
              {[0.16, 0, -0.14].map((d, i) => (
                <span key={i} style={{ width: 46, height: 58, borderRadius: 6, border: "2px solid var(--surface)", background: tile(l.colorHex), filter: `brightness(${1 + d})` }} />
              ))}
            </div>
          </div>

          {/* details */}
          <div style={{ padding: "20px 22px 22px", display: "flex", flexDirection: "column", gap: 14 }}>
            <div style={{ display: "flex", justifyContent: "space-between", alignItems: "flex-start" }}>
              <div>
                <div className="eyebrow" style={{ marginBottom: 4 }}>{l.era} · {l.aesthetic.join(" / ")}</div>
                <h2 className="font-display" style={{ margin: 0, fontSize: 22, fontWeight: 700, letterSpacing: "-.01em", lineHeight: 1.1 }}>{l.brand}</h2>
                <p style={{ margin: "3px 0 0", color: "var(--muted)", fontSize: 13.5 }}>{l.subcategory} · {l.color}{l.size ? ` · Size ${l.size}` : ""}</p>
              </div>
              <button className="icon-act" onClick={closeDetail} aria-label="Close" style={{ boxShadow: "none" }}><Icon.Close size={16} /></button>
            </div>

            {/* price + history */}
            <div className="card" style={{ padding: "12px 13px", display: "flex", justifyContent: "space-between", alignItems: "center", background: "var(--surface-2)", border: "1px solid var(--line)" }}>
              <div>
                <div style={{ display: "flex", alignItems: "baseline", gap: 8 }}>
                  <span className="font-mono tnum" style={{ fontSize: 21, fontWeight: 600 }}>{money(l.price, l.currency)}</span>
                  {drop.dropped && <span className="pill pill-good">▼ {Math.abs(drop.pct)}% from {money(drop.from)}</span>}
                </div>
                <div className="eyebrow" style={{ marginTop: 3 }}>Price history</div>
              </div>
              <Sparkline points={l.priceHistory} good={drop.dropped} w={130} h={40} />
            </div>

            {/* measurement spec sheet */}
            <div>
              <div className="eyebrow" style={{ marginBottom: 7 }}>Measurements · {l.measurements.unit}</div>
              <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: "6px 18px" }}>
                {measures.map((k) => (
                  <div key={k} style={{ display: "flex", justifyContent: "space-between", borderBottom: "1px solid var(--line)", paddingBottom: 4 }}>
                    <span style={{ fontSize: 12.5, color: "var(--muted)" }}>{MEASURE_LABEL[k]}</span>
                    <span className="font-mono tnum" style={{ fontSize: 13, fontWeight: 500 }}>{l.measurements.values[k]}{l.measurements.unit}</span>
                  </div>
                ))}
              </div>
              <p style={{ margin: "8px 0 0", fontSize: 11.5, color: "var(--faint)" }}>Measurements {SRC_NOTE[l.measurements.source_field]}.</p>
            </div>

            {/* why */}
            <div>
              <div className="eyebrow" style={{ marginBottom: 6 }}>Why you're seeing this</div>
              <div style={{ display: "flex", flexWrap: "wrap", gap: 6 }}>
                {l.matchReasons.map((r) => <span key={r} className="pill pill-accent"><Icon.Bolt size={11} /> {r}</span>)}
              </div>
            </div>

            {/* actions */}
            <div style={{ display: "flex", gap: 8, marginTop: 2 }}>
              <a className="btn btn-primary" href={l.listingUrl} target="_blank" rel="noreferrer" style={{ flex: 1 }}>
                Buy on {l.source.replace("shopify:", "")} <Icon.External size={15} />
              </a>
              <button className="btn" aria-pressed={isSaved} onClick={() => toggleFeedback(l.id, "save")} style={{ color: isSaved ? "var(--accent)" : undefined }}>
                <Icon.Bookmark size={16} fill={isSaved} /> {isSaved ? "Saved" : "Save"}
              </button>
              <button className="btn" aria-pressed={isLiked} onClick={() => toggleFeedback(l.id, "like")} style={{ color: isLiked ? "var(--alert)" : undefined }} aria-label="Like">
                <Icon.Heart size={16} fill={isLiked} />
              </button>
            </div>
            <p style={{ margin: 0, fontSize: 11.5, color: "var(--faint)" }}>Listed {timeAgo(l.listedAt)} · availability checked today</p>
          </div>
        </div>

        {/* more like this */}
        {more.length > 0 && (
          <div style={{ borderTop: "1px solid var(--line)", padding: "16px 22px 20px" }}>
            <div className="eyebrow" style={{ marginBottom: 10 }}>More like this</div>
            <div style={{ display: "grid", gridTemplateColumns: "repeat(4, 1fr)", gap: 12 }}>
              {more.slice(0, 4).map((m) => <ItemCard key={m.id} l={m} />)}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
