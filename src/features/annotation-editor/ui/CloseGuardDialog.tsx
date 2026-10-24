// Asked when the window is closing and the annotations could not be saved.
import { useEffect, useRef } from "react";

export interface CloseGuardDialogProps {
  message: string;
  onRetry(): void;
  onDiscard(): void;
}

export function CloseGuardDialog({ message, onRetry, onDiscard }: CloseGuardDialogProps) {
  const retry = useRef<HTMLButtonElement>(null);
  useEffect(() => retry.current?.focus(), []);
  return (
    <div className="editor-dialog-backdrop">
      <div
        className="editor-dialog"
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="close-guard-title"
        aria-describedby="close-guard-message"
      >
        <h2 id="close-guard-title">注釈を保存できませんでした</h2>
        <p id="close-guard-message">{message}</p>
        <div className="editor-dialog-actions">
          <button ref={retry} type="button" onClick={onRetry}>
            再試行
          </button>
          <button type="button" onClick={onDiscard}>
            変更を破棄して閉じる
          </button>
        </div>
      </div>
    </div>
  );
}
