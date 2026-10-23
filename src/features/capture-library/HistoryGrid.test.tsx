import { mockConvertFileSrc } from "@tauri-apps/api/mocks";
import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { HistoryGrid, ROW_HEIGHT, TILE_WIDTH, visibleRows } from "./HistoryGrid";
import { emptySelection } from "./history-selection";
import type { TileActions } from "./HistoryTile";
import { captureFixture, captureId } from "./testing";

const COLUMNS = 4;
const VISIBLE_ROWS = 3;

/** 1,000 captures, newest first, with distinct ids. */
function thousand() {
  return Array.from({ length: 1000 }, (_, i) => {
    const id = `20261022_${String(999 - i).padStart(6, "0")}`;
    return captureFixture(0, {
      id: captureId(id),
      capturedAt: "2026-10-22T17:41:00+09:00",
      thumbnailPath: `C:\\shots\\.snipt\\thumbs\\${id}.png`,
    });
  });
}

function actions() {
  return {
    onSelect: vi.fn<TileActions["onSelect"]>(),
    onOpen: vi.fn<TileActions["onOpen"]>(),
    onMissing: vi.fn<TileActions["onMissing"]>(),
    onContextMenu: vi.fn<TileActions["onContextMenu"]>(),
  };
}

describe("visibleRows", () => {
  it("covers the viewport plus one row on each side", () => {
    expect(visibleRows({ rowCount: 250, scrollTop: 0, viewportHeight: 600 })).toEqual({
      first: 0,
      last: 3,
    });
    expect(visibleRows({ rowCount: 250, scrollTop: ROW_HEIGHT * 10, viewportHeight: 600 })).toEqual(
      { first: 9, last: 13 },
    );
    expect(visibleRows({ rowCount: 2, scrollTop: 0, viewportHeight: 600 })).toEqual({
      first: 0,
      last: 1,
    });
  });
});

describe("HistoryGrid", () => {
  beforeEach(() => {
    mockConvertFileSrc("windows");
    vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockReturnValue(TILE_WIDTH * COLUMNS);
    vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockReturnValue(
      ROW_HEIGHT * VISIBLE_ROWS,
    );
  });

  afterEach(() => vi.restoreAllMocks());

  it("renders only the visible rows of 1,000 captures", () => {
    const items = thousand();
    const { container } = render(
      <HistoryGrid items={items} selection={emptySelection} actions={actions()} />,
    );

    const tiles = screen.getAllByRole("option");
    expect(tiles).toHaveLength((VISIBLE_ROWS + 1) * COLUMNS);

    const grid = screen.getByRole("listbox");
    grid.scrollTop = ROW_HEIGHT * 100;
    fireEvent.scroll(grid);
    const scrolled = screen.getAllByRole("option");
    expect(scrolled).toHaveLength((VISIBLE_ROWS + 2) * COLUMNS);
    // Rows 99..103 are rendered: the first tile is item 99 * COLUMNS.
    expect(container.querySelector("img")).toHaveAttribute(
      "src",
      expect.stringContaining(encodeURIComponent(`${items[99 * COLUMNS]!.id}.png`)),
    );
  });

  it("opens on double click and Enter, and selects on click", () => {
    const items = [captureFixture(2), captureFixture(1)];
    const tileActions = actions();
    const { rerender } = render(
      <HistoryGrid items={items} selection={emptySelection} actions={tileActions} />,
    );
    const tile = screen.getByRole("option", { name: "2026/10/22 17:41:01 1920 × 1080" });

    fireEvent.click(tile, { ctrlKey: true });
    expect(tileActions.onSelect).toHaveBeenCalledWith(items[1]!.id, { ctrl: true, shift: false });
    fireEvent.doubleClick(tile);
    expect(tileActions.onOpen).toHaveBeenCalledWith(items[1]!.id);

    rerender(
      <HistoryGrid
        items={items}
        selection={{ selected: new Set([items[0]!.id]), anchor: items[0]!.id }}
        actions={tileActions}
      />,
    );
    expect(screen.getByRole("option", { name: /17:41:02/ })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    fireEvent.keyDown(screen.getByRole("listbox"), { key: "Enter" });
    expect(tileActions.onOpen).toHaveBeenLastCalledWith(items[0]!.id);
  });

  it("reports a thumbnail that fails to load and shows a placeholder before it exists", () => {
    const items = [captureFixture(2), captureFixture(1, { thumbnailPath: null })];
    const tileActions = actions();
    const { container } = render(
      <HistoryGrid items={items} selection={emptySelection} actions={tileActions} />,
    );

    const images = container.querySelectorAll("img");
    expect(images).toHaveLength(1);
    expect(container.querySelectorAll(".history-thumb-placeholder")).toHaveLength(1);
    fireEvent.error(images[0]!);
    expect(tileActions.onMissing).toHaveBeenCalledWith(items[0]!.id);
  });
});
