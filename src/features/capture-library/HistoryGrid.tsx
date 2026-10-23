// Thumbnail grid with row virtualization: tiles have a fixed size, so only the visible rows and
// one row on each side are rendered (requirement 3.8).
import { type KeyboardEvent, useLayoutEffect, useRef, useState } from "react";
import type { CaptureItem } from "@/shared/ipc/capture-library";
import type { Selection } from "./history-selection";
import { HistoryTile, type TileActions } from "./HistoryTile";

export const TILE_WIDTH = 216;
export const ROW_HEIGHT = 204;
const OVERSCAN_ROWS = 1;

/** The rows to render, inclusive, clamped to the grid. */
export function visibleRows({
  rowCount,
  scrollTop,
  viewportHeight,
}: {
  rowCount: number;
  scrollTop: number;
  viewportHeight: number;
}): { first: number; last: number } {
  const first = Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - OVERSCAN_ROWS);
  const lastVisible = Math.ceil((scrollTop + viewportHeight) / ROW_HEIGHT) - 1;
  const last = Math.min(rowCount - 1, Math.max(lastVisible, 0) + OVERSCAN_ROWS);
  return { first, last };
}

interface Viewport {
  width: number;
  height: number;
  scrollTop: number;
}

export function HistoryGrid({
  items,
  selection,
  actions,
}: {
  items: CaptureItem[];
  selection: Selection;
  actions: TileActions;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [viewport, setViewport] = useState<Viewport>({ width: 0, height: 0, scrollTop: 0 });

  useLayoutEffect(() => {
    const element = ref.current;
    if (element === null) return;
    const measure = () =>
      setViewport({
        width: element.clientWidth,
        height: element.clientHeight,
        scrollTop: element.scrollTop,
      });
    measure();
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  const columns = Math.max(1, Math.floor(viewport.width / TILE_WIDTH));
  const rowCount = Math.ceil(items.length / columns);
  const { first, last } = visibleRows({
    rowCount,
    scrollTop: viewport.scrollTop,
    viewportHeight: viewport.height,
  });

  const onKeyDown = (event: KeyboardEvent) => {
    if (event.key === "Enter" && selection.anchor !== null) {
      event.preventDefault();
      actions.onOpen(selection.anchor);
    }
  };

  const rows = [];
  for (let row = first; row <= last; row++) {
    rows.push(
      <div
        key={row}
        role="presentation"
        className="history-row"
        style={{ top: row * ROW_HEIGHT, height: ROW_HEIGHT }}
      >
        {items.slice(row * columns, (row + 1) * columns).map((item) => (
          <HistoryTile
            key={item.id}
            item={item}
            selected={selection.selected.has(item.id)}
            actions={actions}
          />
        ))}
      </div>,
    );
  }

  return (
    <div
      ref={ref}
      role="listbox"
      aria-label="キャプチャ履歴"
      aria-multiselectable="true"
      tabIndex={0}
      className="history-grid"
      onKeyDown={onKeyDown}
      onScroll={(event) => {
        const scrollTop = event.currentTarget.scrollTop;
        setViewport((current) => ({ ...current, scrollTop }));
      }}
    >
      <div className="history-grid-content" style={{ height: rowCount * ROW_HEIGHT }}>
        {rows}
      </div>
    </div>
  );
}
