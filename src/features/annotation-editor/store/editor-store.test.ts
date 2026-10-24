import { describe, expect, it } from "vitest";
import type { AnnotationDocument, Shape } from "../model/document";
import { DEFAULT_STYLE } from "../model/style";
import { arrow, blank, IMAGE, marker, rect, text } from "../model/testing";
import { createEditorStore, currentDoc } from "./editor-store";

function storeWith(
  shapes: Shape[] = [],
  options: { viewport?: { width: number; height: number } } = {},
) {
  let next = 0;
  const doc: AnnotationDocument = { ...blank(), shapes };
  return createEditorStore({
    doc,
    style: DEFAULT_STYLE,
    viewport: options.viewport ?? { width: 1000, height: 800 },
    newId: () => `new-${++next}`,
  });
}

const shapes = (store: ReturnType<typeof storeWith>) => currentDoc(store.getState()).shapes;

describe("editor store: documents", () => {
  it("records each committed document once and counts revisions", () => {
    const store = storeWith();
    const doc = currentDoc(store.getState());
    store.getState().commitDoc(doc);
    expect(store.getState().revision).toBe(0);

    store.getState().commitDoc({ ...doc, shapes: [arrow("a")] });
    expect(store.getState().revision).toBe(1);
    expect(store.getState().history.past).toHaveLength(1);
    expect(store.getState().saveStatus).toEqual({ kind: "dirty" });
  });

  it("makes one step of the commits of one gesture", () => {
    const store = storeWith([arrow("a"), rect("r")]);
    const doc = currentDoc(store.getState());
    const gesture = {};
    // A first commit that changes nothing makes no step to amend.
    store.getState().commitDoc(doc, gesture);
    store.getState().commitDoc({ ...doc, shapes: [arrow("a")] }, gesture);
    store.getState().commitDoc({ ...doc, shapes: [] }, gesture);
    expect(store.getState().history.past).toEqual([doc]);
    expect(shapes(store)).toEqual([]);

    // Another gesture, an undo, or a commit without one starts a new step.
    store.getState().commitDoc(doc, {});
    expect(store.getState().history.past).toHaveLength(2);
    store.getState().undo();
    store.getState().commitDoc({ ...doc, shapes: [rect("r")] }, gesture);
    expect(store.getState().history.past).toHaveLength(2);
  });

  it("applies a style change to the selection as one step, undone back to the old color", () => {
    const store = storeWith([arrow("a"), rect("r"), text("t")]);
    store.getState().select(["a", "t"]);
    store.getState().applyStyle({ color: "#43A047" });

    expect(shapes(store).map((s) => ("color" in s ? s.color : null))).toEqual([
      "#43A047",
      "#1E88E5",
      "#43A047",
    ]);
    expect(store.getState().history.past).toHaveLength(1);
    expect(store.getState().style.color).toBe("#43A047");

    store.getState().undo();
    expect(shapes(store).map((s) => ("color" in s ? s.color : null))).toEqual([
      "#E53935",
      "#1E88E5",
      "#212121",
    ]);
    expect(store.getState().style.color).toBe("#43A047");
  });

  it("changes only the style of new shapes when nothing is selected", () => {
    const store = storeWith([arrow("a")]);
    store.getState().applyStyle({ strokeWidth: 12 });
    expect(store.getState().style.strokeWidth).toBe(12);
    expect(store.getState().history.past).toHaveLength(0);
    expect(store.getState().revision).toBe(0);
  });

  it("applies each style field only to the shapes that have it", () => {
    const store = storeWith([arrow("a"), text("t"), marker("m", 1)]);
    store.getState().select(["a", "t", "m"]);
    store.getState().applyStyle({ fontSize: 48, textBackground: "color" });
    const [a, t, m] = shapes(store);
    expect(a).toEqual(arrow("a"));
    expect(t).toMatchObject({ fontSize: 48, background: "color" });
    expect(m).toEqual(marker("m", 1));

    // Nothing to change: no step.
    store.getState().applyStyle({ fontSize: 48 });
    expect(store.getState().history.past).toHaveLength(1);
  });

  it("deletes the selection", () => {
    const store = storeWith([arrow("a"), rect("r")]);
    store.getState().deleteSelected();
    expect(store.getState().revision).toBe(0);
    store.getState().select(["a"]);
    store.getState().deleteSelected();
    expect(shapes(store).map((s) => s.id)).toEqual(["r"]);
    expect(store.getState().selection).toEqual([]);
  });

  it("keeps only existing shapes selected after undo and redo", () => {
    const store = storeWith();
    store.getState().commitDoc({ ...currentDoc(store.getState()), shapes: [arrow("a")] });
    store.getState().select(["a"]);
    store.getState().undo();
    expect(store.getState().selection).toEqual([]);
    expect(store.getState().revision).toBe(2);
    store.getState().redo();
    expect(shapes(store).map((s) => s.id)).toEqual(["a"]);
    expect(store.getState().revision).toBe(3);
    // Nothing left to redo.
    store.getState().redo();
    expect(store.getState().revision).toBe(3);
  });
});

describe("editor store: text", () => {
  it("creates a text with the current style", () => {
    const store = storeWith();
    store.getState().applyStyle({ color: "#1E88E5", fontSize: 32, textBackground: "white" });
    store.getState().beginTextEdit({ position: { x: 5, y: 6 } });
    store.getState().updateTextDraft("一行目\n二行目");
    store.getState().commitTextEdit();
    expect(shapes(store)).toEqual([
      {
        id: "new-1",
        kind: "text",
        position: { x: 5, y: 6 },
        text: "一行目\n二行目",
        color: "#1E88E5",
        fontSize: 32,
        background: "white",
      },
    ]);
    expect(store.getState().textEdit).toBeNull();
  });

  it("creates nothing from an empty text", () => {
    const store = storeWith();
    store.getState().beginTextEdit({ position: { x: 5, y: 6 } });
    store.getState().updateTextDraft("  \n ");
    store.getState().commitTextEdit();
    expect(shapes(store)).toEqual([]);
    expect(store.getState().revision).toBe(0);
  });

  it("edits an existing text and deletes it when emptied", () => {
    const store = storeWith([text("t", "前")]);
    store.getState().beginTextEdit({ shapeId: "t" });
    expect(store.getState().textEdit).toEqual({
      shapeId: "t",
      position: { x: 40, y: 40 },
      draft: "前",
    });
    store.getState().updateTextDraft("後");
    store.getState().commitTextEdit();
    expect(shapes(store)).toEqual([text("t", "後")]);

    store.getState().beginTextEdit({ shapeId: "t" });
    store.getState().updateTextDraft("");
    store.getState().commitTextEdit();
    expect(shapes(store)).toEqual([]);
  });

  it("commits the text being typed before starting another", () => {
    const store = storeWith();
    store.getState().beginTextEdit({ position: { x: 1, y: 1 } });
    store.getState().updateTextDraft("a");
    store.getState().beginTextEdit({ position: { x: 2, y: 2 } });
    expect(shapes(store).map((s) => s.id)).toEqual(["new-1"]);
    expect(store.getState().textEdit).toEqual({
      shapeId: null,
      position: { x: 2, y: 2 },
      draft: "",
    });
  });
});

describe("editor store: crop", () => {
  it("crops to the confirmed draft, rounded into the image, and fits it", () => {
    const store = storeWith();
    store.getState().setTool("crop");
    store.getState().beginCrop();
    expect(store.getState().cropDraft).toEqual({ x: 0, y: 0, ...IMAGE });
    store.getState().updateCropDraft({ x: 100.4, y: -20, width: 200, height: 120.6 });
    store.getState().confirmCrop();

    const doc = currentDoc(store.getState());
    expect(doc.crop).toEqual({ x: 100, y: 0, width: 200, height: 101 });
    expect(store.getState().cropDraft).toBeNull();
    expect(store.getState().tool).toBe("select");
    expect(store.getState().view.zoom).toBe(1);
    expect(store.getState().view.pan).toEqual({ x: 500 - 100 - 100, y: (800 - 101) / 2 });
  });

  it("records nothing when cancelled, unchanged or emptied", () => {
    const store = storeWith();
    store.getState().beginCrop();
    store.getState().updateCropDraft({ x: 10, y: 10, width: 50, height: 50 });
    store.getState().cancelCrop();
    store.getState().beginCrop();
    store.getState().confirmCrop();
    store.getState().beginCrop();
    store.getState().updateCropDraft({ x: 10, y: 10, width: 0, height: 50 });
    store.getState().confirmCrop();
    expect(store.getState().revision).toBe(0);
    expect(currentDoc(store.getState()).crop).toBeNull();
  });

  it("clears the crop as an undoable step", () => {
    const store = storeWith();
    store.getState().beginCrop();
    store.getState().updateCropDraft({ x: 10, y: 10, width: 50, height: 50 });
    store.getState().confirmCrop();
    store.getState().clearCrop();
    expect(currentDoc(store.getState()).crop).toBeNull();
    store.getState().undo();
    expect(currentDoc(store.getState()).crop).toEqual({ x: 10, y: 10, width: 50, height: 50 });
  });
});

describe("editor store: view", () => {
  it("starts fitted: large images shrink, small ones show at 100%", () => {
    const large = createEditorStore({
      doc: { ...blank(), image: { width: 4000, height: 2000 } },
      style: DEFAULT_STYLE,
      viewport: { width: 1000, height: 800 },
    });
    expect(large.getState().view.zoom).toBeCloseTo(0.25);
    expect(storeWith().getState().view.zoom).toBe(1);
  });

  it("refits on viewport and DPR changes only in fit mode", () => {
    const store = createEditorStore({
      doc: { ...blank(), image: { width: 2000, height: 1000 } },
      style: DEFAULT_STYLE,
      viewport: { width: 1000, height: 800 },
    });
    expect(store.getState().view.zoom).toBeCloseTo(0.5);
    store.getState().setViewport({ width: 1000, height: 800 }, 1.5);
    expect(store.getState().view.zoom).toBeCloseTo(0.75);

    store.getState().zoomIn();
    expect(store.getState().view).toMatchObject({ zoom: 1, mode: "manual" });
    store.getState().setViewport({ width: 500, height: 400 }, 1);
    expect(store.getState().view.zoom).toBe(1);

    store.getState().zoomFit();
    expect(store.getState().view).toMatchObject({ zoom: 0.25, mode: "fit" });
  });

  it("zooms by steps, to 100% and around an anchor", () => {
    const store = storeWith();
    store.getState().zoomOut({ x: 0, y: 0 });
    expect(store.getState().view.zoom).toBe(0.75);
    store.getState().zoomActual();
    expect(store.getState().view.zoom).toBe(1);
    store.getState().setPan({ x: 3, y: 4 });
    expect(store.getState().view).toEqual({ zoom: 1, pan: { x: 3, y: 4 }, mode: "manual" });
  });
});

describe("editor store: save status", () => {
  it("tracks saves against the revision", () => {
    const store = storeWith();
    const doc = currentDoc(store.getState());
    store.getState().commitDoc({ ...doc, shapes: [arrow("a")] });
    store.getState().saveStarted();
    store.getState().commitDoc({ ...doc, shapes: [arrow("b")] });
    expect(store.getState().saveStatus).toEqual({ kind: "saving" });

    store.getState().saveSucceeded(1);
    expect(store.getState()).toMatchObject({ savedRevision: 1, saveStatus: { kind: "dirty" } });
    store.getState().saveFailed("書き込めません");
    expect(store.getState().saveStatus).toEqual({ kind: "failed", message: "書き込めません" });
    store.getState().saveSucceeded(2);
    expect(store.getState()).toMatchObject({ savedRevision: 2, saveStatus: { kind: "clean" } });
  });
});
