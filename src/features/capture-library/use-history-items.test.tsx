import { emit } from "@tauri-apps/api/event";
import { act, renderHook, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { captureFixture, captureId, mockLibrary } from "./testing";
import { historyReducer, initialHistory, useHistoryItems } from "./use-history-items";

const ids = (items: { id: string }[]) => items.map((item) => item.id);

describe("historyReducer", () => {
  it("ignores a list fetched before a reload", () => {
    const reloaded = historyReducer(initialHistory, { type: "reloaded" });
    const stale = historyReducer(reloaded, {
      type: "loaded",
      generation: 0,
      items: [captureFixture(1)],
    });
    expect(stale.items).toEqual([]);
    expect(stale.loading).toBe(true);
  });

  it("ignores updates for items it does not show", () => {
    const state = { ...initialHistory, items: [captureFixture(1)], loading: false };
    expect(historyReducer(state, { type: "updated", item: captureFixture(2) }).items).toEqual([
      captureFixture(1),
    ]);
  });
});

describe("useHistoryItems", () => {
  it("loads the list and follows library events", async () => {
    const first = [captureFixture(2), captureFixture(1)];
    const second = [captureFixture(9)];
    let lists = 0;
    mockLibrary({ library_list: () => (lists++ === 0 ? first : second) });

    const { result } = renderHook(() => useHistoryItems());
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(ids(result.current.items)).toEqual(["20261022_174102", "20261022_174101"]);

    await act(() => emit("library://item-added", { item: captureFixture(3) }));
    expect(ids(result.current.items)).toEqual([
      "20261022_174103",
      "20261022_174102",
      "20261022_174101",
    ]);

    const updated = captureFixture(2, { hasAnnotations: true });
    await act(() => emit("library://item-updated", { item: updated }));
    expect(result.current.items[1]).toEqual(updated);

    // An id that is already listed is replaced and moved to the top.
    await act(() => emit("library://item-added", { item: captureFixture(1) }));
    expect(ids(result.current.items)).toEqual([
      "20261022_174101",
      "20261022_174103",
      "20261022_174102",
    ]);

    await act(() =>
      emit("library://items-removed", {
        ids: [captureId("20261022_174103")],
        reason: "missingOnDisk",
      }),
    );
    expect(ids(result.current.items)).toEqual(["20261022_174101", "20261022_174102"]);

    await act(() => emit("library://reloaded", { saveDir: "D:\\Elsewhere" }));
    await waitFor(() => expect(ids(result.current.items)).toEqual(["20261022_174109"]));
    expect(lists).toBe(2);
  });
});
