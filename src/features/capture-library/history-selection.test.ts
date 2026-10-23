import { describe, expect, it } from "vitest";
import type { CaptureId } from "@/shared/ipc/capture-library";
import {
  clickSelect,
  contextSelect,
  emptySelection,
  retainListed,
  selectAll,
  selectedInOrder,
  type Selection,
} from "./history-selection";

const order = ["a", "b", "c", "d", "e"] as CaptureId[];
const [a, b, c, d, e] = order as [CaptureId, CaptureId, CaptureId, CaptureId, CaptureId];
const plain = { ctrl: false, shift: false };
const ctrl = { ctrl: true, shift: false };
const shift = { ctrl: false, shift: true };

const ids = (selection: Selection) => selectedInOrder(selection, order);

describe("clickSelect", () => {
  it("selects only the clicked item", () => {
    const once = clickSelect(emptySelection, order, b, plain);
    const twice = clickSelect(once, order, d, plain);
    expect(ids(twice)).toEqual([d]);
    expect(twice.anchor).toBe(d);
  });

  it("toggles with Ctrl", () => {
    let selection = clickSelect(emptySelection, order, b, plain);
    selection = clickSelect(selection, order, d, ctrl);
    expect(ids(selection)).toEqual([b, d]);
    selection = clickSelect(selection, order, b, ctrl);
    expect(ids(selection)).toEqual([d]);
  });

  it("selects a range from the anchor with Shift, in either direction", () => {
    let selection = clickSelect(emptySelection, order, d, plain);
    selection = clickSelect(selection, order, b, shift);
    expect(ids(selection)).toEqual([b, c, d]);
    selection = clickSelect(selection, order, e, shift);
    expect(ids(selection)).toEqual([d, e]);
    expect(selection.anchor).toBe(d);
  });

  it("adds the range with Ctrl+Shift", () => {
    let selection = clickSelect(emptySelection, order, a, plain);
    selection = clickSelect(selection, order, c, ctrl);
    selection = clickSelect(selection, order, e, { ctrl: true, shift: true });
    expect(ids(selection)).toEqual([a, c, d, e]);
  });

  it("treats Shift without an anchor as a plain click", () => {
    expect(ids(clickSelect(emptySelection, order, c, shift))).toEqual([c]);
  });
});

describe("contextSelect", () => {
  it("keeps a selection that contains the item", () => {
    const selection = selectAll(order);
    expect(contextSelect(selection, c)).toBe(selection);
    expect(ids(contextSelect(clickSelect(emptySelection, order, a, plain), c))).toEqual([c]);
  });
});

describe("retainListed", () => {
  it("drops removed ids and the anchor", () => {
    const selection = clickSelect(selectAll(order), order, b, ctrl);
    const remaining = [a, c, e];
    const next = retainListed(selection, remaining);
    expect(selectedInOrder(next, remaining)).toEqual([a, c, e]);
    expect(next.anchor).toBeNull();
  });

  it("keeps the same object when nothing was removed", () => {
    const selection = clickSelect(emptySelection, order, b, plain);
    expect(retainListed(selection, order)).toBe(selection);
  });
});
