// Multi-selection for the history grid, as pure functions over the displayed order.
import type { CaptureId } from "@/shared/ipc/capture-library";

export interface Selection {
  selected: ReadonlySet<CaptureId>;
  /** Where the next Shift+click range starts; also the item Enter opens. */
  anchor: CaptureId | null;
}

export interface Modifiers {
  ctrl: boolean;
  shift: boolean;
}

export const emptySelection: Selection = { selected: new Set(), anchor: null };

/**
 * A click on `id`: alone it selects only `id`, Ctrl toggles it, Shift selects the range from
 * the anchor (Ctrl+Shift adds the range to the selection).
 */
export function clickSelect(
  selection: Selection,
  order: readonly CaptureId[],
  id: CaptureId,
  { ctrl, shift }: Modifiers,
): Selection {
  if (shift && selection.anchor !== null) {
    const from = order.indexOf(selection.anchor);
    const to = order.indexOf(id);
    if (from !== -1 && to !== -1) {
      const range = order.slice(Math.min(from, to), Math.max(from, to) + 1);
      const selected = new Set(ctrl ? selection.selected : []);
      for (const rangeId of range) selected.add(rangeId);
      return { selected, anchor: selection.anchor };
    }
  }
  if (ctrl) {
    const selected = new Set(selection.selected);
    if (selected.has(id)) {
      selected.delete(id);
    } else {
      selected.add(id);
    }
    return { selected, anchor: id };
  }
  return { selected: new Set([id]), anchor: id };
}

/** Right-click: keeps a selection that contains `id`, otherwise selects only `id`. */
export function contextSelect(selection: Selection, id: CaptureId): Selection {
  return selection.selected.has(id) ? selection : { selected: new Set([id]), anchor: id };
}

export function selectAll(order: readonly CaptureId[]): Selection {
  return { selected: new Set(order), anchor: order[0] ?? null };
}

/** Drops ids that are no longer listed. Returns the same object when nothing changed. */
export function retainListed(selection: Selection, order: readonly CaptureId[]): Selection {
  const listed = new Set(order);
  const anchorGone = selection.anchor !== null && !listed.has(selection.anchor);
  const selected = [...selection.selected].filter((id) => listed.has(id));
  if (!anchorGone && selected.length === selection.selected.size) {
    return selection;
  }
  return { selected: new Set(selected), anchor: anchorGone ? null : selection.anchor };
}

/** The selected ids in display order. */
export function selectedInOrder(selection: Selection, order: readonly CaptureId[]): CaptureId[] {
  return order.filter((id) => selection.selected.has(id));
}
