import { act, fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { emptyDocument } from "../model/document";
import { DEFAULT_STYLE } from "../model/style";
import { text } from "../model/testing";
import { ImageSource } from "../services/image-source";
import { EditorContext } from "../store/editor-context";
import { createEditorStore, currentDoc } from "../store/editor-store";
import { TextEditorOverlay } from "./TextEditorOverlay";

function setup(shapes = [text("t", "前")]) {
  const store = createEditorStore({
    doc: { ...emptyDocument({ width: 400, height: 300 }), shapes },
    style: { ...DEFAULT_STYLE, fontSize: 32, textBackground: "color", color: "#1E88E5" },
    viewport: { width: 800, height: 600 },
    devicePixelRatio: 2,
    newId: () => "new",
  });
  render(
    <EditorContext.Provider
      value={{
        store,
        source: new ImageSource({ width: 400, height: 300 } as never),
        newId: () => "new",
      }}
    >
      <TextEditorOverlay />
    </EditorContext.Provider>,
  );
  return store;
}

const box = () => screen.getByRole("textbox", { name: "テキスト" });

describe("TextEditorOverlay", () => {
  it("shows nothing until a text is being typed", () => {
    setup();
    expect(screen.queryByRole("textbox")).toBeNull();
  });

  it("sits at the text's place on screen, in the current style and zoom", () => {
    const store = setup();
    // 100% at DPR 2: the stage scale is 0.5; the image is centered at (300, 225).
    act(() => store.getState().beginTextEdit({ position: { x: 100, y: 40 } }));
    const style = box().style;
    expect([style.left, style.top]).toEqual(["350px", "245px"]);
    expect(style.fontSize).toBe("16px");
    expect(style.color).toBe("rgb(255, 255, 255)");
    expect(style.background).toBe("rgb(30, 136, 229)");
    expect(box()).toHaveFocus();
  });

  it("keeps new lines and commits on blur", () => {
    const store = setup([]);
    act(() => store.getState().beginTextEdit({ position: { x: 10, y: 10 } }));
    fireEvent.change(box(), { target: { value: "一行目\n二行目" } });
    fireEvent.keyDown(box(), { key: "Enter" });
    expect(store.getState().textEdit?.draft).toBe("一行目\n二行目");
    fireEvent.blur(box());
    expect(currentDoc(store.getState()).shapes).toMatchObject([{ text: "一行目\n二行目" }]);
    expect(screen.queryByRole("textbox")).toBeNull();
  });

  it("does not commit on Esc while an IME is composing", () => {
    const store = setup([]);
    act(() => store.getState().beginTextEdit({ position: { x: 10, y: 10 } }));
    fireEvent.compositionStart(box());
    fireEvent.change(box(), { target: { value: "にほんご" } });
    fireEvent.keyDown(box(), { key: "Escape", isComposing: true });
    expect(store.getState().textEdit).not.toBeNull();
    fireEvent.compositionEnd(box());
    fireEvent.change(box(), { target: { value: "日本語" } });
    fireEvent.keyDown(box(), { key: "Escape" });
    expect(store.getState().textEdit).toBeNull();
    expect(currentDoc(store.getState()).shapes).toMatchObject([{ text: "日本語" }]);
  });

  it("edits an existing text in its own style, and removes it when emptied", () => {
    const store = setup();
    act(() => store.getState().beginTextEdit({ shapeId: "t" }));
    expect(box()).toHaveValue("前");
    expect(box().style.background).toBe("rgb(255, 255, 255)");
    fireEvent.change(box(), { target: { value: "" } });
    fireEvent.keyDown(box(), { key: "Escape" });
    expect(currentDoc(store.getState()).shapes).toEqual([]);
  });
});
