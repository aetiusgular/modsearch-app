import { useState } from "react";
import { TopBar } from "../components/AppShell";
import { FilterRail } from "../components/FilterRail";
import { Grid } from "../components/Grid";
import { ActiveFilters } from "../components/ActiveFilters";
import { Icon } from "../components/Icons";
import { useFeed } from "../data/useListings";
import { useApp } from "../state/store";

export function ForYou() {
  const rows = useFeed();
  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100%" }}>
      <TopBar title="For You" subtitle="Ranked by your taste across every source you've added" />
      <div style={{ display: "flex", flex: 1, minHeight: 0 }}>
        <FilterRail />
        <main className="scroll-y" style={{ flex: 1, padding: "18px 22px 48px" }}>
          <ActiveFilters count={rows?.length ?? null} />
          <Grid rows={rows} />
        </main>
      </div>
    </div>
  );
}

export function Search() {
  const { searchText, setSearchText, setQuery, query } = useApp();
  const [local, setLocal] = useState(searchText);
  const rows = useFeed();
  const submit = (e: React.FormEvent) => { e.preventDefault(); setSearchText(local); };
  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100%" }}>
      <header style={{ borderBottom: "1px solid var(--line)", padding: "14px 22px", background: "var(--surface)", position: "sticky", top: 0, zIndex: 5 }}>
        <form onSubmit={submit} style={{ display: "flex", gap: 8, alignItems: "center", maxWidth: 640 }}>
          <div style={{ position: "relative", flex: 1 }}>
            <span style={{ position: "absolute", left: 11, top: "50%", transform: "translateY(-50%)", color: "var(--faint)" }}><Icon.Search size={16} /></span>
            <input type="search" value={local} onChange={(e) => setLocal(e.target.value)}
              placeholder="Search black wool overcoat, Margiela GAT, 90s techwear…"
              style={{ paddingLeft: 34 }} aria-label="Search" />
          </div>
          <button className="btn btn-primary" type="submit">Search</button>
        </form>
        <p style={{ margin: "8px 0 0", fontSize: 12, color: "var(--muted)" }}>Results are re-ranked by your taste, not raw keyword relevance.</p>
      </header>
      <div style={{ display: "flex", flex: 1, minHeight: 0 }}>
        <FilterRail />
        <main className="scroll-y" style={{ flex: 1, padding: "18px 22px 48px" }}>
          <ActiveFilters count={rows?.length ?? null} />
          {!searchText && !Object.keys(query).length ? (
            <div style={{ color: "var(--muted)", padding: "40px 0", textAlign: "center" }}>
              <p className="font-display" style={{ fontSize: 17, fontWeight: 700, color: "var(--ink)", margin: "0 0 6px" }}>Search everything you've indexed</p>
              <p style={{ margin: 0, fontSize: 13 }}>Type a query or use the filters. Your taste model orders what comes back.</p>
            </div>
          ) : <Grid rows={rows} />}
        </main>
      </div>
    </div>
  );
}
