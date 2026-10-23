import { useEffect, useRef } from "react";

/** Asks before moving `count` captures to the recycle bin (requirement 4.1). */
export function DeleteConfirmDialog({
  count,
  onConfirm,
  onCancel,
}: {
  count: number;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  const confirmRef = useRef<HTMLButtonElement>(null);
  useEffect(() => confirmRef.current?.focus(), []);

  return (
    <div className="history-backdrop">
      <div
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="delete-confirm-message"
        className="history-dialog"
        onKeyDown={(event) => {
          if (event.key === "Escape") {
            event.stopPropagation();
            onCancel();
          }
        }}
      >
        <p id="delete-confirm-message">{count} 件のキャプチャをごみ箱へ移動しますか?</p>
        <div className="history-dialog-buttons">
          <button ref={confirmRef} type="button" onClick={onConfirm}>
            ごみ箱へ移動
          </button>
          <button type="button" onClick={onCancel}>
            キャンセル
          </button>
        </div>
      </div>
    </div>
  );
}
