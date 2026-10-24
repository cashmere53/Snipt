// The editor window itself: its close button, the requests Rust sends it, and destroying it.
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import {
  CLOSE_REQUEST_EVENT,
  FLUSH_REQUEST_EVENT,
  type FlushRequestPayload,
} from "@/shared/ipc/annotation-editor";

export type Unlisten = () => void;

export interface WindowPort {
  /** The user closes the window; without `preventDefault` it is destroyed. */
  onCloseRequested(handler: (event: { preventDefault(): void }) => void): Promise<Unlisten>;
  /** Rust closes the window from outside (its capture is deleted); save, then close anyway. */
  onCloseRequest(handler: () => void): Promise<Unlisten>;
  /** The app is about to quit: save, keep the window, answer with `editor_flush_result`. */
  onFlushRequest(handler: (payload: FlushRequestPayload) => void): Promise<Unlisten>;
  destroy(): Promise<void>;
}

export function tauriWindowPort(): WindowPort {
  const window = getCurrentWebviewWindow();
  return {
    onCloseRequested: (handler) => window.onCloseRequested(handler),
    onCloseRequest: (handler) => window.listen(CLOSE_REQUEST_EVENT, () => handler()),
    onFlushRequest: (handler) =>
      window.listen<FlushRequestPayload>(FLUSH_REQUEST_EVENT, (event) => handler(event.payload)),
    destroy: () => window.destroy(),
  };
}
