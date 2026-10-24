// The editor's keys. Nothing applies while a text input has the focus or an IME is composing,
// and the WebView's own reload, print and find keys are stopped.
import { useEffect } from "react";
import type { StoreApi } from "zustand/vanilla";
import type { EditorStore, ToolId } from "../store/editor-store";
import { useEditorContext } from "../store/editor-context";

export type Shortcut =
  | { kind: "tool"; tool: ToolId }
  | { kind: "delete" }
  | { kind: "undo" }
  | { kind: "redo" }
  | { kind: "copy" }
  | { kind: "saveAs" }
  | { kind: "zoomIn" }
  | { kind: "zoomOut" }
  | { kind: "zoomFit" }
  | { kind: "zoomActual" }
  | { kind: "enter" }
  | { kind: "escape" }
  /** A WebView default (reload, print, find...) to stop. */
  | { kind: "block" };

/** By physical key, so they work whatever the layout or IME mode. */
const TOOL_KEYS: Record<string, ToolId> = {
  KeyV: "select",
  KeyA: "arrow",
  KeyR: "rect",
  KeyT: "text",
  KeyB: "mosaic",
  KeyC: "crop",
  KeyN: "marker",
};

const BLOCKED_WITH_CTRL = new Set([
  "KeyR",
  "KeyP",
  "KeyF",
  "KeyG",
  "KeyU",
  "KeyJ",
  "KeyH",
  "KeyO",
  "KeyN",
  "KeyS",
]);

type KeyInput = Pick<KeyboardEvent, "key" | "code" | "ctrlKey" | "shiftKey" | "altKey" | "metaKey">;

export function shortcutFor(e: KeyInput): Shortcut | null {
  const ctrl = e.ctrlKey || e.metaKey;
  if (e.key === "F5" || e.key === "F3" || e.key === "F7" || e.key === "F12")
    return { kind: "block" };
  if (e.altKey) return null;
  if (ctrl) {
    switch (e.code) {
      case "KeyZ":
        return e.shiftKey ? { kind: "redo" } : { kind: "undo" };
      case "KeyY":
        return { kind: "redo" };
      case "KeyC":
        return e.shiftKey ? null : { kind: "copy" };
      case "KeyS":
        return e.shiftKey ? { kind: "saveAs" } : { kind: "block" };
      case "Equal":
      case "NumpadAdd":
      case "Semicolon":
        return { kind: "zoomIn" };
      case "Minus":
      case "NumpadSubtract":
        return { kind: "zoomOut" };
      case "Digit0":
      case "Numpad0":
        return { kind: "zoomFit" };
      case "Digit1":
      case "Numpad1":
        return { kind: "zoomActual" };
    }
    if (e.key === "+") return { kind: "zoomIn" };
    return BLOCKED_WITH_CTRL.has(e.code) ? { kind: "block" } : null;
  }
  if (e.key === "Delete" || e.key === "Backspace") return { kind: "delete" };
  if (e.key === "Enter") return { kind: "enter" };
  if (e.key === "Escape") return { kind: "escape" };
  const tool = e.shiftKey ? undefined : TOOL_KEYS[e.code];
  return tool === undefined ? null : { kind: "tool", tool };
}

export interface ShortcutHandlers {
  onCopy(): void;
  onSaveAs(): void;
}

export function runShortcut(
  shortcut: Shortcut,
  store: StoreApi<EditorStore>,
  handlers: ShortcutHandlers,
): void {
  const state = store.getState();
  switch (shortcut.kind) {
    case "tool":
      return state.setTool(shortcut.tool);
    case "delete":
      return state.deleteSelected();
    case "undo":
      return state.undo();
    case "redo":
      return state.redo();
    case "copy":
      return handlers.onCopy();
    case "saveAs":
      return handlers.onSaveAs();
    case "zoomIn":
      return state.zoomIn();
    case "zoomOut":
      return state.zoomOut();
    case "zoomFit":
      return state.zoomFit();
    case "zoomActual":
      return state.zoomActual();
    case "enter":
      if (state.cropDraft !== null) state.confirmCrop();
      return;
    case "escape":
      if (state.cropDraft !== null) state.cancelCrop();
      else state.select([]);
      return;
    case "block":
      return;
  }
}

function isTyping(target: EventTarget | null): boolean {
  return (
    target instanceof HTMLElement &&
    (target.isContentEditable || ["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName))
  );
}

export function useShortcuts(handlers: ShortcutHandlers): void {
  const { store } = useEditorContext();
  const { onCopy, onSaveAs } = handlers;
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      const shortcut = shortcutFor(e);
      if (shortcut === null) return;
      if (shortcut.kind === "block") {
        e.preventDefault();
        return;
      }
      if (e.isComposing || e.key === "Process" || isTyping(e.target)) return;
      // Enter and Esc only matter while cropping or with a selection; let buttons have them.
      if (
        (shortcut.kind === "enter" || shortcut.kind === "escape") &&
        e.target instanceof HTMLButtonElement
      ) {
        return;
      }
      e.preventDefault();
      runShortcut(shortcut, store, { onCopy, onSaveAs });
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [store, onCopy, onSaveAs]);
}
