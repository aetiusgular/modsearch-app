import type { DataClient } from "./client";
import type { FacetCounts, FeedQuery, Listing } from "./types";

// A19: DataClient over the extension bridge. When the SPA runs as the full-page
// extension app, requests go chrome.runtime.sendMessage -> service worker ->
// native messaging -> the Rust engine, and back, correlated by a per-request id.
// The engine already returns the Listing/FacetCounts shape, so no mapping is needed.

// Minimal ambient shape to avoid a hard dependency on @types/chrome.
const cr: any = (globalThis as any).chrome;
let seq = 1;

function call<T>(type: string, extra: Record<string, unknown> = {}): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    const id = seq++;
    cr.runtime.sendMessage({ id, type, ...extra }, (resp: any) => {
      const err = cr.runtime.lastError;
      if (err) return reject(new Error(err.message));
      if (!resp || !resp.ok) return reject(new Error(resp?.error ?? "engine error"));
      resolve(resp.data as T);
    });
  });
}

export class EngineDataClient implements DataClient {
  getFeed(query: FeedQuery, hidden: Set<string>) {
    return call<Listing[]>("getFeed", { query, hidden: [...hidden] });
  }
  getItem(id: string) {
    return call<Listing | undefined>("getItem", { itemId: id });
  }
  moreLikeThis(id: string, hidden: Set<string>) {
    return call<Listing[]>("moreLikeThis", { itemId: id, hidden: [...hidden] });
  }
  getFacets() {
    return call<FacetCounts>("getFacets");
  }
  getSaved(savedIds: Set<string>) {
    return call<Listing[]>("getSaved", { savedIds: [...savedIds] });
  }
  getDrops(hidden: Set<string>) {
    return call<Listing[]>("getDrops", { hidden: [...hidden] });
  }
  async recordFeedback(id: string, kind: "like" | "save" | "hide") {
    await call("recordFeedback", { itemId: id, feedbackKind: kind });
  }
}
