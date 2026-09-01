import { useEffect, useState } from "react";
import { client } from "../data/client";
import { CONDITION_LABEL, MEASURE_LABEL, type Condition, type FacetCounts, type MeasureKey } from "../data/types";
import { useApp } from "../state/store";
import { money } from "./util";

const MEASURES_SHOWN: MeasureKey[] = ["pit_to_pit", "shoulder", "length", "sleeve", "waist", "inseam", "rise"];

function Section({ title, defaultOpen = true, children }: { title: string; defaultOpen?: boolean; children: React.ReactNode }) {
  const [open, setOpen] = useState(defaultOpen);
  return (
    <div style={{ borderTop: "1px solid var(--line)", padding: "12px 0" }}>
      <button className="btn-ghost" onClick={() => setOpen(!open)}
        style={{ display: "flex", width: "100%", justifyContent: "space-between", alignItems: "center", padding: "2px 0", cursor: "pointer" }}>
        <span className="eyebrow">{title}</span>
        <span style={{ color: "var(--faint)", fontFamily: "var(--font-mono)", fontSize: 12 }}>{open ? "–" : "+"}</span>
      </button>
      {open && <div style={{ marginTop: 10, display: "flex", flexDirection: "column", gap: 8 }}>{children}</div>}
    </div>
  );
}

function ChipToggle<T extends string>({ options, selected, onToggle, render }: {
  options: [T, number][]; selected: T[]; onToggle: (v: T) => void; render?: (v: T) => React.ReactNode;
}) {
  return (
    <div style={{ display: "flex", flexWrap: "wrap", gap: 6 }}>
      {options.map(([v, n]) => (
        <button key={v} className={`chip ${selected.includes(v) ? "chip-active" : ""}`} onClick={() => onToggle(v)}>
          {render ? render(v) : v}
          <span style={{ opacity: 0.6, fontFamily: "var(--font-mono)", fontSize: 11 }}>{n}</span>
        </button>
      ))}
    </div>
  );
}

function RangeControl({ label, unit, bounds, value, onChange }: {
  label: string; unit?: string; bounds: [number, number]; value?: [number, number]; onChange: (v: [number, number] | undefined) => void;
}) {
  const [lo, hi] = value ?? bounds;
  const active = !!value;
  const set = (nlo: number, nhi: number) => {
    const clampedLo = Math.min(nlo, nhi);
    const clampedHi = Math.max(nlo, nhi);
    if (clampedLo <= bounds[0] && clampedHi >= bounds[1]) onChange(undefined);
    else onChange([clampedLo, clampedHi]);
  };
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 5 }}>
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "baseline" }}>
        <span style={{ fontSize: 12.5, color: active ? "var(--ink)" : "var(--muted)", fontWeight: active ? 600 : 500 }}>{label}</span>
        <span className="font-mono tnum" style={{ fontSize: 11, color: active ? "var(--accent)" : "var(--faint)" }}>
          {lo}{unit} – {hi}{unit}
        </span>
      </div>
      <div style={{ position: "relative", height: 16, display: "grid", alignItems: "center" }}>
        <input type="range" min={bounds[0]} max={bounds[1]} value={lo} onChange={(e) => set(+e.target.value, hi)}
          style={{ gridArea: "1 / 1" }} aria-label={`${label} minimum`} />
        <input type="range" min={bounds[0]} max={bounds[1]} value={hi} onChange={(e) => set(lo, +e.target.value)}
          style={{ gridArea: "1 / 1" }} aria-label={`${label} maximum`} />
      </div>
    </div>
  );
}

export function FilterRail() {
  const { query, setQuery, resetFilters } = useApp();
  const [f, setF] = useState<FacetCounts | null>(null);
  useEffect(() => { client.getFacets().then(setF); }, []);
  if (!f) return null;

  const toggle = <T extends string>(key: "sizes" | "colors" | "brands" | "conditions" | "categories" | "eras", v: T) => {
    const cur = (query[key] as T[] | undefined) ?? [];
    const next = cur.includes(v) ? cur.filter((x) => x !== v) : [...cur, v];
    setQuery({ [key]: next.length ? next : undefined } as any);
  };

  const activeCount =
    (query.brands?.length ?? 0) + (query.colors?.length ?? 0) + (query.categories?.length ?? 0) +
    (query.conditions?.length ?? 0) + (query.eras?.length ?? 0) + (query.sizes?.length ?? 0) +
    (query.priceMin != null || query.priceMax != null ? 1 : 0) +
    Object.keys(query.measures ?? {}).length;

  return (
    <aside className="scroll-y" style={{ width: 268, flexShrink: 0, borderRight: "1px solid var(--line)", padding: "16px 16px 40px", height: "100%" }}>
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 4 }}>
        <span className="font-display" style={{ fontWeight: 700, fontSize: 15 }}>Filters</span>
        {activeCount > 0 && (
          <button className="btn btn-ghost" style={{ fontSize: 12, padding: "3px 8px", color: "var(--accent)" }} onClick={resetFilters}>
            Clear {activeCount}
          </button>
        )}
      </div>

      <Section title="Measurements (cm)">
        <p style={{ fontSize: 11.5, color: "var(--faint)", margin: "0 0 4px" }}>Filter by the garment's real dimensions, not a labeled size.</p>
        {MEASURES_SHOWN.filter((k) => f.measureRanges[k]).map((k) => (
          <RangeControl key={k} label={MEASURE_LABEL[k]} unit="" bounds={f.measureRanges[k]!}
            value={query.measures?.[k]}
            onChange={(v) => {
              const m = { ...query.measures };
              if (v) m[k] = v; else delete m[k];
              setQuery({ measures: Object.keys(m).length ? m : undefined });
            }} />
        ))}
      </Section>

      <Section title="Price">
        <RangeControl label="Price" bounds={f.priceRange}
          value={query.priceMin != null || query.priceMax != null ? [query.priceMin ?? f.priceRange[0], query.priceMax ?? f.priceRange[1]] : undefined}
          onChange={(v) => setQuery({ priceMin: v ? v[0] : undefined, priceMax: v ? v[1] : undefined })} />
        <div style={{ display: "flex", justifyContent: "space-between", fontSize: 11, color: "var(--faint)" }}>
          <span className="font-mono">{money(f.priceRange[0])}</span><span className="font-mono">{money(f.priceRange[1])}</span>
        </div>
      </Section>

      <Section title="Size">
        <ChipToggle options={f.sizes as [string, number][]} selected={query.sizes ?? []} onToggle={(v) => toggle("sizes", v)} />
      </Section>

      <Section title="Color">
        <div style={{ display: "flex", flexWrap: "wrap", gap: 7 }}>
          {f.colors.map(([name, hex, n]) => {
            const on = (query.colors ?? []).includes(name);
            return (
              <button key={name} className="chip" onClick={() => toggle("colors", name)}
                title={`${name} (${n})`}
                style={{ padding: "4px 9px 4px 5px", borderColor: on ? "var(--accent)" : "var(--line-strong)", background: on ? "var(--accent-soft)" : "var(--surface)" }}>
                <span style={{ width: 13, height: 13, borderRadius: 4, background: hex, border: "1px solid rgba(0,0,0,.15)" }} />
                <span style={{ fontSize: 12 }}>{name}</span>
              </button>
            );
          })}
        </div>
      </Section>

      <Section title="Brand" defaultOpen={false}>
        <ChipToggle options={f.brands as [string, number][]} selected={query.brands ?? []} onToggle={(v) => toggle("brands", v)} />
      </Section>

      <Section title="Category">
        <ChipToggle options={f.categories as [string, number][]} selected={query.categories ?? []} onToggle={(v) => toggle("categories", v)} />
      </Section>

      <Section title="Condition" defaultOpen={false}>
        <ChipToggle options={f.conditions as [Condition, number][]} selected={query.conditions ?? []} onToggle={(v) => toggle("conditions", v)}
          render={(c) => CONDITION_LABEL[c]} />
      </Section>

      <Section title="Era" defaultOpen={false}>
        <ChipToggle options={f.eras as [string, number][]} selected={query.eras ?? []} onToggle={(v) => toggle("eras", v)} />
      </Section>
    </aside>
  );
}
