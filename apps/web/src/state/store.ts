import { create } from "zustand";
import type { FeedQuery, FeedbackKind, SortKey } from "../data/types";

// In-memory UI state. No browser storage here: the rendered preview cannot rely
// on it, and in the real app the engine is the source of truth for saved items
// and taste. Per-viewer conveniences (theme) can move to a guarded store later.

type Theme = "light" | "dark" | "system";

interface AppState {
  theme: Theme;
  setTheme: (t: Theme) => void;

  saved: Set<string>;
  liked: Set<string>;
  hidden: Set<string>;
  toggleFeedback: (id: string, kind: FeedbackKind) => void;

  query: FeedQuery;
  setQuery: (patch: Partial<FeedQuery>) => void;
  resetFilters: () => void;
  sort: SortKey;
  setSort: (s: SortKey) => void;
  searchText: string;
  setSearchText: (s: string) => void;

  detailId: string | null;
  openDetail: (id: string) => void;
  closeDetail: () => void;

  railOpen: boolean;
  toggleRail: () => void;

  // bumped whenever feedback changes so views re-query
  rev: number;
}

const applyTheme = (t: Theme) => {
  const root = document.documentElement;
  if (t === "system") root.removeAttribute("data-theme");
  else root.setAttribute("data-theme", t);
};

const clone = (s: Set<string>) => new Set(s);

export const useApp = create<AppState>((set, get) => ({
  theme: "system",
  setTheme: (t) => { applyTheme(t); set({ theme: t }); },

  saved: new Set(),
  liked: new Set(),
  hidden: new Set(),
  toggleFeedback: (id, kind) => {
    const key = kind === "save" ? "saved" : kind === "like" ? "liked" : "hidden";
    const next = clone(get()[key] as Set<string>);
    next.has(id) ? next.delete(id) : next.add(id);
    // hiding an item clears any like/save on it
    const patch: Partial<AppState> = { [key]: next, rev: get().rev + 1 } as Partial<AppState>;
    if (kind === "hide" && next.has(id)) {
      const s = clone(get().saved); s.delete(id);
      const l = clone(get().liked); l.delete(id);
      patch.saved = s; patch.liked = l;
    }
    client_record(id, kind);
    set(patch);
  },

  query: {},
  setQuery: (patch) => set({ query: { ...get().query, ...patch } }),
  resetFilters: () => set({ query: {}, searchText: "" }),
  sort: "match",
  setSort: (s) => set({ sort: s }),
  searchText: "",
  setSearchText: (s) => set({ searchText: s }),

  detailId: null,
  openDetail: (id) => set({ detailId: id }),
  closeDetail: () => set({ detailId: null }),

  railOpen: true,
  toggleRail: () => set({ railOpen: !get().railOpen }),

  rev: 0,
}));

// fire-and-forget feedback to the data client (no-op in mock)
import { client } from "../data/client";
function client_record(id: string, kind: FeedbackKind) {
  void client.recordFeedback(id, kind);
}
