// The overlay window's side of the events Rust sends it.
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import {
  ACTIVATE_EVENT,
  DEACTIVATE_EVENT,
  HIGHLIGHT_EVENT,
  type OverlayActivate,
  type OverlayDeactivate,
  type OverlayHighlight,
} from "@/shared/ipc/screen-capture";

export type Unlisten = () => void;

export interface OverlayPort {
  onActivate(handler: (payload: OverlayActivate) => void): Promise<Unlisten>;
  onHighlight(handler: (payload: OverlayHighlight) => void): Promise<Unlisten>;
  onDeactivate(handler: (payload: OverlayDeactivate) => void): Promise<Unlisten>;
}

export function tauriOverlayPort(): OverlayPort {
  const window = getCurrentWebviewWindow();
  return {
    onActivate: (handler) =>
      window.listen<OverlayActivate>(ACTIVATE_EVENT, (event) => handler(event.payload)),
    onHighlight: (handler) =>
      window.listen<OverlayHighlight>(HIGHLIGHT_EVENT, (event) => handler(event.payload)),
    onDeactivate: (handler) =>
      window.listen<OverlayDeactivate>(DEACTIVATE_EVENT, (event) => handler(event.payload)),
  };
}
