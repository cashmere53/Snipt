// The history window: toolbar, thumbnail grid, context menu and the delete confirmation.
import { type KeyboardEvent, useCallback, useMemo, useState } from "react";
import {
  type CaptureId,
  type LibraryError,
  copyCapture,
  deleteCaptures,
  openInEditor,
  pruneMissing,
  revealCapture,
} from "@/shared/ipc/capture-library";
import { DeleteConfirmDialog } from "./DeleteConfirmDialog";
import { HistoryGrid } from "./HistoryGrid";
import {
  clickSelect,
  contextSelect,
  emptySelection,
  retainListed,
  type Selection,
  selectAll,
  selectedInOrder,
} from "./history-selection";
import type { TileActions } from "./HistoryTile";
import { useHistoryItems } from "./use-history-items";
import "./history.css";

function errorMessage(error: unknown): string {
  if (typeof error === "object" && error !== null && "message" in error) {
    return String((error as LibraryError).message);
  }
  return String(error);
}

export function HistoryWindow() {
  const { items, loading } = useHistoryItems();
  const order = useMemo(() => items.map((item) => item.id), [items]);
  const [storedSelection, setSelection] = useState<Selection>(emptySelection);
  // Captures removed elsewhere leave the selection too.
  const selection = useMemo(() => retainListed(storedSelection, order), [storedSelection, order]);
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const [confirming, setConfirming] = useState<CaptureId[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  const selected = selectedInOrder(selection, order);
  const single = selected.length === 1 ? selected[0]! : null;

  const run = useCallback((action: Promise<unknown>) => {
    setError(null);
    action.catch((reason: unknown) => setError(errorMessage(reason)));
  }, []);

  const copy = () => single !== null && run(copyCapture(single));
  const reveal = () => single !== null && run(revealCapture(single));
  const askDelete = () => selected.length > 0 && setConfirming(selected);

  const confirmDelete = async () => {
    const ids = confirming ?? [];
    setConfirming(null);
    setError(null);
    try {
      const outcome = await deleteCaptures(ids);
      if (outcome.failed.length > 0) {
        const reason = outcome.failed[0]!.error.message;
        setError(`${outcome.failed.length} 件を削除できませんでした: ${reason}`);
      }
    } catch (reason) {
      setError(errorMessage(reason));
    }
  };

  const actions: TileActions = useMemo(
    () => ({
      onSelect: (id, modifiers) => {
        setMenu(null);
        setSelection((current) => clickSelect(retainListed(current, order), order, id, modifiers));
      },
      onOpen: (id) => run(openInEditor(id)),
      onMissing: (id) => void pruneMissing([id]).catch(() => {}),
      onContextMenu: (id, x, y) => {
        setSelection((current) => contextSelect(retainListed(current, order), id));
        setMenu({ x, y });
      },
    }),
    [order, run],
  );

  const onKeyDown = (event: KeyboardEvent) => {
    if (confirming !== null) return;
    if (event.key === "Delete") {
      event.preventDefault();
      askDelete();
    } else if (event.key === "a" && (event.ctrlKey || event.metaKey)) {
      event.preventDefault();
      setSelection(selectAll(order));
    } else if (event.key === "Escape") {
      setMenu(null);
    }
  };

  const menuItem = (label: string, onClick: () => void, enabled: boolean) => (
    <button
      type="button"
      role="menuitem"
      disabled={!enabled}
      onClick={() => {
        setMenu(null);
        onClick();
      }}
    >
      {label}
    </button>
  );

  return (
    <main className="history" data-testid="HistoryWindow" onKeyDown={onKeyDown}>
      <div role="toolbar" aria-label="履歴の操作" className="history-toolbar">
        <button type="button" onClick={copy} disabled={single === null}>
          コピー
        </button>
        <button type="button" onClick={reveal} disabled={single === null}>
          エクスプローラーで表示
        </button>
        <button type="button" onClick={askDelete} disabled={selected.length === 0}>
          削除
        </button>
        <span className="history-count">
          {selected.length > 0 ? `${selected.length} 件を選択中 / ` : ""}
          {items.length} 件
        </span>
      </div>
      {error !== null && (
        <p role="alert" className="history-error">
          {error}
        </p>
      )}
      {!loading && items.length === 0 ? (
        <p className="history-empty">キャプチャはまだありません</p>
      ) : (
        <HistoryGrid items={items} selection={selection} actions={actions} />
      )}
      {menu !== null && (
        <div
          role="menu"
          aria-label="キャプチャの操作"
          className="history-menu"
          style={{ left: menu.x, top: menu.y }}
        >
          {menuItem(
            "編集画面で開く",
            () => single !== null && run(openInEditor(single)),
            single !== null,
          )}
          {menuItem("コピー", copy, single !== null)}
          {menuItem("エクスプローラーで表示", reveal, single !== null)}
          {menuItem("削除", askDelete, selected.length > 0)}
        </div>
      )}
      {confirming !== null && (
        <DeleteConfirmDialog
          count={confirming.length}
          onConfirm={() => void confirmDelete()}
          onCancel={() => setConfirming(null)}
        />
      )}
    </main>
  );
}
