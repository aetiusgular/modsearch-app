import { NavLink, useLocation } from "react-router-dom";
import { Icon } from "./Icons";
import { useApp } from "../state/store";
import type { SortKey } from "../data/types";

const NAV = [
  { to: "/", label: "For You", icon: Icon.ForYou, end: true },
  { to: "/search", label: "Search", icon: Icon.Search },
  { to: "/saved", label: "Saved", icon: Icon.Saved, count: "saved" as const },
  { to: "/drops", label: "Drops", icon: Icon.Drops, count: "drops" as const },
  { to: "/settings", label: "Settings", icon: Icon.Settings },
];

const SORTS: [SortKey, string][] = [
  ["match", "Best match"], ["newest", "Newest"], ["price_asc", "Price ↑"], ["price_desc", "Price ↓"],
];

function ThemeToggle() {
  const { theme, setTheme } = useApp();
  const next = theme === "dark" ? "light" : "dark";
  return (
    <button className="btn btn-ghost" style={{ width: "100%", justifyContent: "flex-start", gap: 11, color: "var(--muted)" }}
      onClick={() => setTheme(next)} aria-label={`Switch to ${next} theme`}>
      {theme === "dark" ? <Icon.Sun size={17} /> : <Icon.Moon size={17} />}
      {theme === "dark" ? "Light" : "Dark"} mode
    </button>
  );
}

export function Sidebar() {
  const { saved, rev } = useApp();
  void rev;
  return (
    <nav style={{ width: 214, flexShrink: 0, borderRight: "1px solid var(--line)", padding: "16px 12px", display: "flex", flexDirection: "column", height: "100%", background: "var(--surface)" }}>
      <div style={{ padding: "4px 8px 16px", display: "flex", alignItems: "center", gap: 9 }}>
        <span style={{ width: 22, height: 22, borderRadius: 6, background: "var(--accent)", display: "grid", placeItems: "center", color: "var(--accent-ink)" }}>
          <Icon.ForYou size={13} />
        </span>
        <span className="font-display" style={{ fontWeight: 800, fontSize: 17, letterSpacing: "-.02em" }}>ModSearch</span>
      </div>
      <div style={{ display: "flex", flexDirection: "column", gap: 3 }}>
        {NAV.map((n) => (
          <NavLink key={n.to} to={n.to} end={n.end} className="nav-item">
            <n.icon size={17} />
            {n.label}
            {n.count === "saved" && saved.size > 0 && <span className="nav-count">{saved.size}</span>}
          </NavLink>
        ))}
      </div>
      <div style={{ marginTop: "auto", display: "flex", flexDirection: "column", gap: 3 }}>
        <div className="card" style={{ padding: "10px 11px", marginBottom: 8, background: "var(--surface-2)", border: "1px solid var(--line)" }}>
          <div className="eyebrow" style={{ marginBottom: 3 }}>Local-first</div>
          <p style={{ margin: 0, fontSize: 11.5, color: "var(--muted)", lineHeight: 1.45 }}>Your taste model runs on this device. Nothing is sold or shared.</p>
        </div>
        <ThemeToggle />
      </div>
    </nav>
  );
}

export function TopBar({ title, subtitle, showSort = true }: { title: string; subtitle?: string; showSort?: boolean }) {
  const { sort, setSort } = useApp();
  const loc = useLocation();
  const searching = loc.pathname.startsWith("/search");
  return (
    <header style={{ borderBottom: "1px solid var(--line)", padding: "14px 22px", display: "flex", alignItems: "center", justifyContent: "space-between", gap: 16, background: "var(--surface)", position: "sticky", top: 0, zIndex: 5 }}>
      <div>
        <h1 className="font-display" style={{ margin: 0, fontSize: 19, fontWeight: 700, letterSpacing: "-.01em", textWrap: "balance" as any }}>{title}</h1>
        {subtitle && <p style={{ margin: "2px 0 0", fontSize: 12.5, color: "var(--muted)" }}>{subtitle}</p>}
      </div>
      {showSort && !searching && (
        <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
          <span className="eyebrow">Sort</span>
          <div style={{ display: "flex", gap: 4 }}>
            {SORTS.map(([k, label]) => (
              <button key={k} className={`chip ${sort === k ? "chip-active" : ""}`} onClick={() => setSort(k)} style={{ fontSize: 11.5, padding: "4px 9px" }}>{label}</button>
            ))}
          </div>
        </div>
      )}
    </header>
  );
}
