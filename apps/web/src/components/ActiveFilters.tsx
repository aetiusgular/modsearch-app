import { CONDITION_LABEL, MEASURE_LABEL, type Condition, type MeasureKey } from "../data/types";
import { useApp } from "../state/store";
import { Icon } from "./Icons";
import { money } from "./util";

export function ActiveFilters({ count }: { count: number | null }) {
  const { query, setQuery } = useApp();
  const chips: { label: string; clear: () => void }[] = [];

  const removeFrom = (key: "brands" | "colors" | "categories" | "conditions" | "eras" | "sizes", v: string) => {
    const cur = (query[key] as string[] | undefined) ?? [];
    const next = cur.filter((x) => x !== v);
    setQuery({ [key]: next.length ? next : undefined } as any);
  };

  (query.brands ?? []).forEach((b) => chips.push({ label: b, clear: () => removeFrom("brands", b) }));
  (query.colors ?? []).forEach((c) => chips.push({ label: c, clear: () => removeFrom("colors", c) }));
  (query.categories ?? []).forEach((c) => chips.push({ label: c, clear: () => removeFrom("categories", c) }));
  (query.sizes ?? []).forEach((s) => chips.push({ label: `Size ${s}`, clear: () => removeFrom("sizes", s) }));
  (query.conditions ?? []).forEach((c) => chips.push({ label: CONDITION_LABEL[c as Condition], clear: () => removeFrom("conditions", c) }));
  (query.eras ?? []).forEach((e) => chips.push({ label: e, clear: () => removeFrom("eras", e) }));
  if (query.priceMin != null || query.priceMax != null)
    chips.push({ label: `${money(query.priceMin ?? 0)} – ${query.priceMax != null ? money(query.priceMax) : "∞"}`, clear: () => setQuery({ priceMin: undefined, priceMax: undefined }) });
  Object.entries(query.measures ?? {}).forEach(([k, r]) => {
    if (!r) return;
    chips.push({ label: `${MEASURE_LABEL[k as MeasureKey]} ${r[0]}–${r[1]}cm`, clear: () => setQuery({ measures: { ...query.measures, [k]: undefined } }) });
  });

  return (
    <div style={{ display: "flex", alignItems: "center", flexWrap: "wrap", gap: 8, marginBottom: 16 }}>
      {count !== null && (
        <span className="font-mono tnum" style={{ fontSize: 12, color: "var(--muted)" }}>{count} item{count === 1 ? "" : "s"}</span>
      )}
      {chips.length > 0 && <span style={{ width: 1, height: 16, background: "var(--line)" }} />}
      {chips.map((c, i) => (
        <button key={i} className="chip chip-active" onClick={c.clear} style={{ fontSize: 11.5, padding: "3px 6px 3px 10px" }}>
          {c.label} <Icon.Close size={12} />
        </button>
      ))}
    </div>
  );
}
