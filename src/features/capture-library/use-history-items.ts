// The history list: loaded once, then kept current from library events.
import { useEffect, useReducer } from "react";
import {
  type CaptureId,
  type CaptureItem,
  listCaptures,
  onLibraryEvent,
} from "@/shared/ipc/capture-library";

export interface HistoryState {
  items: CaptureItem[];
  loading: boolean;
  /** Bumped by `library://reloaded` to fetch the list again. */
  generation: number;
}

export type HistoryAction =
  | { type: "loaded"; generation: number; items: CaptureItem[] }
  | { type: "added"; item: CaptureItem }
  | { type: "updated"; item: CaptureItem }
  | { type: "removed"; ids: CaptureId[] }
  | { type: "reloaded" };

export const initialHistory: HistoryState = { items: [], loading: true, generation: 0 };

export function historyReducer(state: HistoryState, action: HistoryAction): HistoryState {
  switch (action.type) {
    case "loaded":
      // A list fetched before a reload belongs to the previous folder.
      return action.generation === state.generation
        ? { ...state, items: action.items, loading: false }
        : state;
    case "added":
      return {
        ...state,
        items: [action.item, ...state.items.filter((item) => item.id !== action.item.id)],
      };
    case "updated":
      return {
        ...state,
        items: state.items.map((item) => (item.id === action.item.id ? action.item : item)),
      };
    case "removed": {
      const removed = new Set(action.ids);
      return { ...state, items: state.items.filter((item) => !removed.has(item.id)) };
    }
    case "reloaded":
      return { items: [], loading: true, generation: state.generation + 1 };
  }
}

export function useHistoryItems(): HistoryState {
  const [state, dispatch] = useReducer(historyReducer, initialHistory);

  useEffect(() => {
    let active = true;
    const unlisteners: (() => void)[] = [];
    const subscriptions = [
      onLibraryEvent("library://item-added", ({ item }) => dispatch({ type: "added", item })),
      onLibraryEvent("library://item-updated", ({ item }) => dispatch({ type: "updated", item })),
      onLibraryEvent("library://items-removed", ({ ids }) => dispatch({ type: "removed", ids })),
      onLibraryEvent("library://reloaded", () => dispatch({ type: "reloaded" })),
    ];
    for (const subscription of subscriptions) {
      void subscription.then((unlisten) => {
        if (active) {
          unlisteners.push(unlisten);
        } else {
          unlisten();
        }
      });
    }
    return () => {
      active = false;
      for (const unlisten of unlisteners) unlisten();
    };
  }, []);

  const { generation } = state;
  useEffect(() => {
    let current = true;
    listCaptures().then(
      (items) => {
        if (current) dispatch({ type: "loaded", generation, items });
      },
      (error: unknown) => {
        console.error("library_list failed", error);
        if (current) dispatch({ type: "loaded", generation, items: [] });
      },
    );
    return () => {
      current = false;
    };
  }, [generation]);

  return state;
}
