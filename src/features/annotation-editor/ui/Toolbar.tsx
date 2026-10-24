// Tools, copy and save, undo and redo, and the crop buttons. Copying and saving are the
// window's (`onCopy`, `onSaveAs`).
import { canRedo, canUndo } from "../model/history";
import type { ToolId } from "../store/editor-store";
import { currentDoc } from "../store/editor-store";
import { useEditor, useEditorContext } from "../store/editor-context";

export const TOOLS: readonly { id: ToolId; label: string; key: string }[] = [
  { id: "select", label: "選択", key: "V" },
  { id: "arrow", label: "矢印", key: "A" },
  { id: "rect", label: "四角", key: "R" },
  { id: "text", label: "テキスト", key: "T" },
  { id: "mosaic", label: "ぼかし", key: "B" },
  { id: "crop", label: "切り抜き", key: "C" },
  { id: "marker", label: "番号", key: "N" },
];

export interface ToolbarProps {
  onCopy(): void;
  onSaveAs(): void;
}

export function Toolbar({ onCopy, onSaveAs }: ToolbarProps) {
  const { store } = useEditorContext();
  const tool = useEditor((s) => s.tool);
  const undoable = useEditor((s) => canUndo(s.history));
  const redoable = useEditor((s) => canRedo(s.history));
  const cropping = useEditor((s) => s.cropDraft !== null);
  const cropped = useEditor((s) => currentDoc(s).crop !== null);
  const actions = store.getState();

  return (
    <div className="editor-toolbar" role="toolbar" aria-label="ツール">
      <div className="editor-tool-group">
        {TOOLS.map(({ id, label, key }) => (
          <button
            key={id}
            type="button"
            aria-pressed={tool === id}
            title={`${label} (${key})`}
            onClick={() => actions.setTool(id)}
          >
            {label}
          </button>
        ))}
      </div>
      {cropping ? (
        <div className="editor-tool-group">
          <button type="button" title="切り抜きを確定 (Enter)" onClick={actions.confirmCrop}>
            確定
          </button>
          <button type="button" title="切り抜きを取り消す (Esc)" onClick={actions.cancelCrop}>
            取消
          </button>
        </div>
      ) : null}
      {cropped && !cropping ? (
        <div className="editor-tool-group">
          <button type="button" onClick={actions.clearCrop}>
            切り抜き解除
          </button>
        </div>
      ) : null}
      <div className="editor-tool-group">
        <button type="button" title="元に戻す (Ctrl+Z)" disabled={!undoable} onClick={actions.undo}>
          元に戻す
        </button>
        <button type="button" title="やり直す (Ctrl+Y)" disabled={!redoable} onClick={actions.redo}>
          やり直す
        </button>
      </div>
      <div className="editor-tool-group editor-tool-group-end">
        <button type="button" title="コピー (Ctrl+C)" onClick={onCopy}>
          コピー
        </button>
        <button type="button" title="名前を付けて保存 (Ctrl+Shift+S)" onClick={onSaveAs}>
          名前を付けて保存
        </button>
      </div>
    </div>
  );
}
