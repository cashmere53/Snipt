// A rendered editor canvas for tests: a 400×300 image in an 800×600 view (so 100%, centered at
// (200, 150)), with helpers to press, move and release the mouse at image points.
import { act, render } from "@testing-library/react";
import Konva from "konva";
import { afterEach, beforeEach, vi } from "vitest";
import type { AnnotationDocument, Point } from "../model/document";
import { emptyDocument } from "../model/document";
import { DEFAULT_STYLE } from "../model/style";
import { ImageSource } from "../services/image-source";
import { EditorContext } from "../store/editor-context";
import { createEditorStore } from "../store/editor-store";
import { EditorCanvas } from "./EditorCanvas";

export const VIEW = { width: 800, height: 600 };
export const IMAGE = { width: 400, height: 300 };
/** Where image point (0, 0) is in the view at 100%. */
export const ORIGIN = { x: 200, y: 150 };

/** Gives every element the view's size (jsdom lays nothing out). */
export function useViewSize(): void {
  beforeEach(() => {
    vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockReturnValue(VIEW.width);
    vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockReturnValue(VIEW.height);
  });
  afterEach(() => {
    // Destroying removes the stage from the list: iterate over a copy.
    for (const stage of Konva.stages.slice()) stage.destroy();
  });
}

/** The client position of image point `at`. */
const client = (at: Point) => ({ clientX: ORIGIN.x + at.x, clientY: ORIGIN.y + at.y });

/** A striped gray image of `IMAGE`'s size. */
export function grayImage(): HTMLCanvasElement {
  const canvas = document.createElement("canvas");
  canvas.width = IMAGE.width;
  canvas.height = IMAGE.height;
  const context = canvas.getContext("2d")!;
  for (let x = 0; x < IMAGE.width; x += 2) {
    context.fillStyle = x % 4 === 0 ? "#808080" : "#c0c0c0";
    context.fillRect(x, 0, 2, IMAGE.height);
  }
  return canvas;
}

export function renderCanvas(doc: Partial<AnnotationDocument> = {}) {
  let next = 0;
  const newId = () => `id-${++next}`;
  const store = createEditorStore({
    doc: { ...emptyDocument(IMAGE), ...doc },
    style: DEFAULT_STYLE,
    newId,
  });
  const source = new ImageSource(grayImage());
  const result = render(
    <EditorContext.Provider value={{ store, source, newId }}>
      <EditorCanvas />
    </EditorContext.Provider>,
  );
  const stage = Konva.stages.at(-1)!;

  const fire = (target: EventTarget, type: string, at: Point, init: MouseEventInit = {}) =>
    act(() => {
      // Hit testing reads the hit canvas, which Konva otherwise redraws on the next frame.
      stage.draw();
      target.dispatchEvent(
        new MouseEvent(type, { bubbles: true, button: 0, ...client(at), ...init }),
      );
    });

  return {
    ...result,
    store,
    stage,
    state: () => store.getState(),
    /** Presses on the stage at image point `at`. */
    down: (at: Point, init?: MouseEventInit) => fire(stage.content, "mousedown", at, init),
    /** Moves over the window: a drag in progress follows it. */
    move: (at: Point, init?: MouseEventInit) => fire(window, "mousemove", at, init),
    /** Releases on the stage (Konva's clicks need that); it reaches the window too. */
    up: (at: Point, init?: MouseEventInit) => fire(stage.content, "mouseup", at, init),
    /** A press, move and release on the stage, as Konva's own drags need them. */
    stageDrag: (from: Point, to: Point) => {
      fire(stage.content, "mousedown", from);
      fire(stage.content, "mousemove", to);
      fire(window, "mousemove", to);
      fire(stage.content, "mouseup", to);
    },
    wheel: (init: WheelEventInit) =>
      act(() => {
        stage.content.dispatchEvent(
          new WheelEvent("wheel", {
            bubbles: true,
            cancelable: true,
            ...client({ x: 0, y: 0 }),
            ...init,
          }),
        );
      }),
  };
}
