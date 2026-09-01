import { useState } from "react";
import { TopBar } from "../components/AppShell";
import { MEASURE_LABEL, type MeasureKey } from "../data/types";
import { useApp } from "../state/store";

const AESTHETICS = ["Techwear", "Americana", "Avant-Garde", "Workwear", "Minimalism", "Gorpcore", "Archive", "Ivy", "Y2K", "Military"];
const PROFILE_MEASURES: MeasureKey[] = ["pit_to_pit", "shoulder", "length", "sleeve", "waist", "inseam", "rise"];
const STORAGE_MODES = [
  ["keep", "Keep all history", "Full long-tail taste evolution. Largest on disk."],
  ["core", "Structural core only", "Drop raw image vectors, keep brands, shops, and category preferences."],
  ["purge", "Complete purge", "Reset to a clean cold start. Cannot be undone."],
] as const;

function Card({ title, desc, children }: { title: string; desc?: string; children: React.ReactNode }) {
  return (
    <section className="card" style={{ padding: "16px 18px", display: "flex", flexDirection: "column", gap: 12 }}>
      <div>
        <h2 className="font-display" style={{ margin: 0, fontSize: 15.5, fontWeight: 700 }}>{title}</h2>
        {desc && <p style={{ margin: "3px 0 0", fontSize: 12.5, color: "var(--muted)" }}>{desc}</p>}
      </div>
      {children}
    </section>
  );
}

export function Settings() {
  const { theme, setTheme } = useApp();
  const [aes, setAes] = useState<string[]>(["Archive", "Techwear"]);
  const [profile, setProfile] = useState<Partial<Record<MeasureKey, string>>>({ pit_to_pit: "54", shoulder: "46", length: "72" });
  const [emailDrops, setEmailDrops] = useState(true);
  const [storage, setStorage] = useState<"keep" | "core" | "purge">("keep");
  const toggleAes = (a: string) => setAes((p) => (p.includes(a) ? p.filter((x) => x !== a) : [...p, a]));

  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100%" }}>
      <TopBar title="Settings" subtitle="Taste, fit, alerts, and what stays on your device." showSort={false} />
      <main className="scroll-y" style={{ flex: 1, padding: "20px 22px 48px" }}>
        <div style={{ display: "flex", flexDirection: "column", gap: 14, maxWidth: 620 }}>

          <Card title="Taste" desc="Seed your For You feed. The model refines this as you browse.">
            <div style={{ display: "flex", flexWrap: "wrap", gap: 7 }}>
              {AESTHETICS.map((a) => (
                <button key={a} className={`chip ${aes.includes(a) ? "chip-active" : ""}`} onClick={() => toggleAes(a)}>{a}</button>
              ))}
            </div>
          </Card>

          <Card title="Your measurements" desc="Used to flag what actually fits. Stored on your device only.">
            <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fill, minmax(150px, 1fr))", gap: 10 }}>
              {PROFILE_MEASURES.map((k) => (
                <label key={k} style={{ display: "flex", flexDirection: "column", gap: 4 }}>
                  <span style={{ fontSize: 12, color: "var(--muted)" }}>{MEASURE_LABEL[k]} <span style={{ color: "var(--faint)" }}>cm</span></span>
                  <input type="number" value={profile[k] ?? ""} onChange={(e) => setProfile((p) => ({ ...p, [k]: e.target.value }))} placeholder="—" />
                </label>
              ))}
            </div>
          </Card>

          <Card title="Alerts">
            <label style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: 10 }}>
              <span style={{ fontSize: 13.5 }}>Email me when a saved item drops in price</span>
              <button role="switch" aria-checked={emailDrops} onClick={() => setEmailDrops(!emailDrops)}
                style={{ width: 42, height: 24, borderRadius: 99, border: "1px solid var(--line-strong)", background: emailDrops ? "var(--accent)" : "var(--surface-2)", position: "relative", cursor: "pointer", transition: "background .15s" }}>
                <span style={{ position: "absolute", top: 2, left: emailDrops ? 20 : 2, width: 18, height: 18, borderRadius: 99, background: "var(--surface)", boxShadow: "var(--shadow-sm)", transition: "left .15s" }} />
              </button>
            </label>
            <p style={{ margin: 0, fontSize: 12, color: "var(--faint)" }}>Drops always appear on the Drops board too, whether or not email is on.</p>
          </Card>

          <Card title="Data & storage" desc="Everything lives on this device. Choose how much to keep.">
            <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
              {STORAGE_MODES.map(([id, label, note]) => (
                <label key={id} style={{ display: "flex", gap: 10, alignItems: "flex-start", cursor: "pointer", padding: "9px 11px", borderRadius: 8, border: `1px solid ${storage === id ? "var(--accent)" : "var(--line)"}`, background: storage === id ? "var(--accent-soft)" : "var(--surface)" }}>
                  <input type="radio" name="storage" checked={storage === id} onChange={() => setStorage(id)} style={{ marginTop: 2, accentColor: "var(--accent)" }} />
                  <span>
                    <span style={{ fontSize: 13.5, fontWeight: 600, color: id === "purge" ? "var(--alert)" : "var(--ink)" }}>{label}</span>
                    <span style={{ display: "block", fontSize: 12, color: "var(--muted)", marginTop: 1 }}>{note}</span>
                  </span>
                </label>
              ))}
            </div>
          </Card>

          <Card title="Appearance">
            <div style={{ display: "flex", gap: 6 }}>
              {(["system", "light", "dark"] as const).map((t) => (
                <button key={t} className={`chip ${theme === t ? "chip-active" : ""}`} onClick={() => setTheme(t)} style={{ textTransform: "capitalize" }}>{t}</button>
              ))}
            </div>
          </Card>

        </div>
      </main>
    </div>
  );
}
