// Zoom controls and the save status, with a retry after a failed save (`onRetry`).
import { useEditor, useEditorContext } from "../store/editor-context";
import type { SaveStatus } from "../store/editor-store";

export function saveStatusText(status: SaveStatus): string {
  switch (status.kind) {
    case "clean":
      return "保存済み";
    case "dirty":
      return "未保存の変更があります";
    case "saving":
      return "保存中…";
    case "failed":
      return `保存できませんでした: ${status.message}`;
  }
}

export function StatusBar({ onRetry }: { onRetry(): void }) {
  const { store } = useEditorContext();
  const zoom = useEditor((s) => s.view.zoom);
  const status = useEditor((s) => s.saveStatus);
  const actions = store.getState();

  return (
    <div className="editor-status" role="status">
      <div className="editor-zoom">
        <button
          type="button"
          aria-label="縮小"
          title="縮小 (Ctrl+-)"
          onClick={() => actions.zoomOut()}
        >
          −
        </button>
        <span aria-label="表示倍率">{Math.round(zoom * 100)}%</span>
        <button
          type="button"
          aria-label="拡大"
          title="拡大 (Ctrl+=)"
          onClick={() => actions.zoomIn()}
        >
          +
        </button>
        <button type="button" title="100% (Ctrl+1)" onClick={actions.zoomActual}>
          100%
        </button>
        <button type="button" title="全体を表示 (Ctrl+0)" onClick={actions.zoomFit}>
          全体
        </button>
      </div>
      <div className={`editor-save editor-save-${status.kind}`}>
        <span>{saveStatusText(status)}</span>
        {status.kind === "failed" ? (
          <button type="button" onClick={onRetry}>
            再試行
          </button>
        ) : null}
      </div>
    </div>
  );
}
