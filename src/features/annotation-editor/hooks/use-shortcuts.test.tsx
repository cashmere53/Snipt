import { act, fireEvent } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { arrow } from "../model/testing";
import { currentDoc } from "../store/editor-store";
import { renderWithEditor } from "../store/testing";
import { shortcutFor, useShortcuts } from "./use-shortcuts";

function Keys({ onCopy, onSaveAs }: { onCopy(): void; onSaveAs(): void }) {
  useShortcuts({ onCopy, onSaveAs });
  return <textarea aria-label="入力" />;
}

const key = (init: Partial<KeyboardEventInit> & { key: string; code?: string }) =>
  fireEvent.keyDown(window, { code: "", ...init });

function setup() {
  const onCopy = vi.fn<() => void>();
  const onSaveAs = vi.fn<() => void>();
  const f = renderWithEditor(<Keys onCopy={onCopy} onSaveAs={onSaveAs} />, {
    doc: { shapes: [arrow("a")] },
  });
  return { ...f, onCopy, onSaveAs };
}

describe("shortcutFor", () => {
  it("maps the documented keys", () => {
    const none = { ctrlKey: false, shiftKey: false, altKey: false, metaKey: false };
    const ctrl = { ...none, ctrlKey: true };
    const cases: [Parameters<typeof shortcutFor>[0], string][] = [
      [{ ...none, key: "v", code: "KeyV" }, "tool:select"],
      [{ ...none, key: "Process", code: "KeyB" }, "tool:mosaic"],
      [{ ...none, key: "n", code: "KeyN" }, "tool:marker"],
      [{ ...none, key: "Delete", code: "Delete" }, "delete"],
      [{ ...none, key: "Backspace", code: "Backspace" }, "delete"],
      [{ ...ctrl, key: "z", code: "KeyZ" }, "undo"],
      [{ ...ctrl, key: "y", code: "KeyY" }, "redo"],
      [{ ...ctrl, shiftKey: true, key: "Z", code: "KeyZ" }, "redo"],
      [{ ...ctrl, key: "c", code: "KeyC" }, "copy"],
      [{ ...ctrl, shiftKey: true, key: "S", code: "KeyS" }, "saveAs"],
      [{ ...ctrl, key: "=", code: "Equal" }, "zoomIn"],
      [{ ...ctrl, key: "+", code: "Semicolon" }, "zoomIn"],
      [{ ...ctrl, key: "-", code: "Minus" }, "zoomOut"],
      [{ ...ctrl, key: "0", code: "Digit0" }, "zoomFit"],
      [{ ...ctrl, key: "1", code: "Digit1" }, "zoomActual"],
      [{ ...none, key: "Enter", code: "Enter" }, "enter"],
      [{ ...none, key: "Escape", code: "Escape" }, "escape"],
      [{ ...none, key: "F5", code: "F5" }, "block"],
      [{ ...ctrl, key: "r", code: "KeyR" }, "block"],
      [{ ...ctrl, key: "p", code: "KeyP" }, "block"],
      [{ ...ctrl, key: "f", code: "KeyF" }, "block"],
    ];
    expect(
      cases.map(([event]) => {
        const shortcut = shortcutFor(event);
        return shortcut?.kind === "tool" ? `tool:${shortcut.tool}` : (shortcut?.kind ?? null);
      }),
    ).toEqual(cases.map(([, expected]) => expected));
    expect(shortcutFor({ ...none, key: "x", code: "KeyX" })).toBeNull();
    expect(shortcutFor({ ...none, altKey: true, key: "v", code: "KeyV" })).toBeNull();
  });
});

describe("useShortcuts", () => {
  it("runs the editor actions", () => {
    const f = setup();
    key({ key: "r", code: "KeyR" });
    expect(f.state().tool).toBe("rect");
    key({ key: "v", code: "KeyV" });
    act(() => f.state().select(["a"]));
    key({ key: "Delete" });
    expect(currentDoc(f.state()).shapes).toEqual([]);
    key({ key: "z", code: "KeyZ", ctrlKey: true });
    expect(currentDoc(f.state()).shapes).toEqual([arrow("a")]);
    key({ key: "c", code: "KeyC", ctrlKey: true });
    key({ key: "S", code: "KeyS", ctrlKey: true, shiftKey: true });
    expect([f.onCopy.mock.calls.length, f.onSaveAs.mock.calls.length]).toEqual([1, 1]);
    key({ key: "=", code: "Equal", ctrlKey: true });
    expect(f.state().view.zoom).toBe(1.25);
  });

  it("confirms and cancels crops with Enter and Esc, and Esc clears the selection", () => {
    const f = setup();
    key({ key: "c", code: "KeyC" });
    act(() => f.state().updateCropDraft({ x: 0, y: 0, width: 100, height: 100 }));
    key({ key: "Enter" });
    expect(currentDoc(f.state()).crop).toEqual({ x: 0, y: 0, width: 100, height: 100 });
    key({ key: "c", code: "KeyC" });
    key({ key: "Escape" });
    expect(f.state().cropDraft).toBeNull();
    act(() => f.state().select(["a"]));
    key({ key: "Escape" });
    expect(f.state().selection).toEqual([]);
  });

  it("leaves keys typed into inputs and during IME composition alone", () => {
    const f = setup();
    act(() => f.state().select(["a"]));
    const input = f.getByLabelText("入力");
    fireEvent.keyDown(input, { key: "Delete", code: "Delete" });
    fireEvent.keyDown(input, { key: "c", code: "KeyC", ctrlKey: true });
    key({ key: "Delete", isComposing: true });
    expect(currentDoc(f.state()).shapes).toEqual([arrow("a")]);
    expect(f.onCopy).not.toHaveBeenCalled();
  });

  it("stops the WebView's reload and find", () => {
    setup();
    const reload = new KeyboardEvent("keydown", { key: "F5", code: "F5", cancelable: true });
    const find = new KeyboardEvent("keydown", {
      key: "f",
      code: "KeyF",
      ctrlKey: true,
      cancelable: true,
    });
    window.dispatchEvent(reload);
    window.dispatchEvent(find);
    expect([reload.defaultPrevented, find.defaultPrevented]).toEqual([true, true]);
  });
});

describe("useShortcuts cleanup", () => {
  it("stops listening when unmounted", () => {
    const f = setup();
    f.unmount();
    key({ key: "r", code: "KeyR" });
    expect(f.state().tool).toBe("select");
  });
});
