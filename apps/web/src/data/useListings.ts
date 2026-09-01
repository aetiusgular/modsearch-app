import { useEffect, useState } from "react";
import { client } from "./client";
import type { FeedQuery, Listing } from "./types";
import { useApp } from "../state/store";

export function useFeed(extra?: Partial<FeedQuery>) {
  const { query, sort, searchText, hidden, rev } = useApp();
  const [rows, setRows] = useState<Listing[] | null>(null);
  useEffect(() => {
    let live = true;
    const q: FeedQuery = { ...query, ...extra, sort, text: searchText || query.text };
    client.getFeed(q, hidden).then((r) => { if (live) setRows(r); });
    return () => { live = false; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [JSON.stringify(query), sort, searchText, rev, JSON.stringify(extra)]);
  return rows;
}

export function useSaved() {
  const { saved, rev } = useApp();
  const [rows, setRows] = useState<Listing[] | null>(null);
  useEffect(() => { let l = true; client.getSaved(saved).then((r) => l && setRows(r)); return () => { l = false; }; }, [saved, rev]);
  return rows;
}

export function useDrops() {
  const { hidden, rev } = useApp();
  const [rows, setRows] = useState<Listing[] | null>(null);
  useEffect(() => { let l = true; client.getDrops(hidden).then((r) => l && setRows(r)); return () => { l = false; }; }, [hidden, rev]);
  return rows;
}
