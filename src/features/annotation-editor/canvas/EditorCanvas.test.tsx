import { act } from "@testing-library/react";
import type Konva from "konva";
import { beforeAll, describe, expect, it } from "vitest";
import { CANVAS_WARM_UP_TIMEOUT, warmUpCanvas } from "@/test/canvas";
import type { MosaicShape } from "../model/document";
import { arrow, marker, rect, text } from "../model/testing";
import { currentDoc } from "../store/editor-store";
import { renderCanvas, useViewSize } from "./testing";

const shapes = (f: ReturnType<typeof renderCanvas>) => currentDoc(f.state()).shapes;

const mosaic: MosaicShape = {
  id: "m",
  kind: "mosaic",
  rect: { x: 0, y: 0, width: 32, height: 32 },
  blockSize: 16,
};

describe("EditorCanvas", () => {
  beforeAll(warmUpCanvas, CANVAS_WARM_UP_TIMEOUT);
  useViewSize();

  it("shows the image at its initial zoom and every shape", () => {
    const f = renderCanvas({ shapes: [arrow("a"), rect("r"), text("t"), marker("n", 1), mosaic] });
    expect(f.state().view).toMatchObject({ zoom: 1, pan: { x: 200, y: 150 } });
    expect([f.stage.width(), f.stage.height()]).toEqual([800, 600]);
    expect(f.stage.find(".shape").map((node) => node.id())).toEqual(["m", "a", "r", "t", "n"]);
    // Mosaics are drawn on the layer below the other shapes.
    const [, mosaics, others] = f.stage.getLayers();
    expect(mosaics!.findOne("#m")).toBeDefined();
    expect(others!.findOne("#m")).toBeUndefined();
    const blocks = mosaics!.findOne<Konva.Image>("#m")!.image() as HTMLCanvasElement;
    expect([blocks.width, blocks.height]).toEqual([32, 32]);
  });

  it("clips the view to the crop, except while cropping", () => {
    const crop = { x: 10, y: 20, width: 100, height: 50 };
    const f = renderCanvas({ crop, shapes: [rect("r")] });
    const clips = () => f.stage.find<Konva.Group>("Group").map((group) => group.clip());
    expect(clips().slice(0, 3)).toEqual([crop, crop, crop]);

    act(() => f.state().setTool("crop"));
    expect(
      clips()
        .slice(0, 3)
        .every((clip) => clip.width === undefined),
    ).toBe(true);
    const frame = f.stage.findOne(".crop-frame")!;
    expect(frame.getAttrs()).toMatchObject(crop);
  });

  it("draws a rectangle per drag and keeps the tool", () => {
    const f = renderCanvas();
    act(() => f.state().setTool("rect"));
    f.down({ x: 10, y: 10 });
    f.move({ x: 60, y: 40 });
    expect(f.stage.find("Rect").length).toBeGreaterThan(0);
    expect(shapes(f)).toEqual([]);
    f.up({ x: 60, y: 40 });

    expect(shapes(f)).toEqual([
      {
        id: "id-1",
        kind: "rect",
        rect: { x: 10, y: 10, width: 50, height: 30 },
        color: "#E53935",
        strokeWidth: 4,
      },
    ]);
    expect(f.state().history.past).toHaveLength(1);
    expect(f.state().tool).toBe("rect");
  });

  it("snaps arrows to 45° with Shift", () => {
    const f = renderCanvas();
    act(() => f.state().setTool("arrow"));
    f.down({ x: 10, y: 10 });
    f.up({ x: 50, y: 14 }, { shiftKey: true });
    expect(shapes(f)).toMatchObject([{ kind: "arrow", end: { x: 50, y: 10 } }]);
  });

  it("places markers and starts texts with a click", () => {
    const f = renderCanvas({ shapes: [marker("n", 4)] });
    act(() => f.state().setTool("marker"));
    f.down({ x: 100, y: 100 });
    expect(shapes(f).at(-1)).toMatchObject({
      kind: "marker",
      number: 5,
      center: { x: 100, y: 100 },
    });

    act(() => f.state().setTool("text"));
    f.down({ x: 30, y: 40 });
    expect(f.state().textEdit).toEqual({ shapeId: null, position: { x: 30, y: 40 }, draft: "" });
    // Clicks outside the image make nothing.
    act(() => f.state().commitTextEdit());
    act(() => f.state().setTool("marker"));
    f.down({ x: -50, y: 10 });
    expect(shapes(f)).toHaveLength(2);
  });

  it("keeps the focus on a text box opened by a click, but not for markers", () => {
    const f = renderCanvas();
    // Whether the mousedown's default action (moving the focus) was cancelled by the stage.
    const defaultPrevented = (tool: "text" | "marker", at: { x: number; y: number }) => {
      act(() => f.state().setTool(tool));
      let prevented = false;
      const listen = (e: Event) => {
        prevented = e.defaultPrevented;
      };
      f.stage.content.addEventListener("mousedown", listen);
      // The events are cancelable, as a real mouse press is.
      f.down(at, { cancelable: true });
      f.stage.content.removeEventListener("mousedown", listen);
      return prevented;
    };

    expect(defaultPrevented("text", { x: 30, y: 40 })).toBe(true);
    act(() => f.state().commitTextEdit());
    expect(defaultPrevented("marker", { x: 100, y: 100 })).toBe(false);
  });

  it("sets the crop draft by dragging with the crop tool", () => {
    const f = renderCanvas();
    act(() => f.state().setTool("crop"));
    f.down({ x: 350.4, y: -20 });
    f.up({ x: 500, y: 100 });
    expect(f.state().cropDraft).toEqual({ x: 350, y: 0, width: 50, height: 100 });
    expect(currentDoc(f.state()).crop).toBeNull();
  });

  it("selects with a press, deselects on the background, and moves in one step", () => {
    const f = renderCanvas({
      shapes: [{ ...rect("r"), rect: { x: 20, y: 20, width: 100, height: 60 } }],
    });
    // On the top edge, away from the handles.
    f.down({ x: 40, y: 20 });
    f.up({ x: 40, y: 20 });
    expect(f.state().selection).toEqual(["r"]);

    f.stageDrag({ x: 40, y: 20 }, { x: 70, y: 40 });
    expect(shapes(f)[0]).toMatchObject({ rect: { x: 50, y: 40, width: 100, height: 60 } });
    expect(f.state().history.past).toHaveLength(1);

    f.down({ x: 300, y: 250 });
    expect(f.state().selection).toEqual([]);
  });

  it("resizes rectangles with the handles in one step", () => {
    const f = renderCanvas({
      shapes: [{ ...rect("r"), rect: { x: 20, y: 20, width: 100, height: 60 } }],
    });
    act(() => f.state().select(["r"]));
    // What a handle drag leaves on the node: a scale, then the end of the transform.
    const transformer = f.stage.findOne<Konva.Transformer>("Transformer")!;
    expect(transformer.nodes().map((node) => node.id())).toEqual(["r"]);
    act(() => {
      transformer.nodes()[0]!.scale({ x: 1.3, y: 80 / 60 });
      transformer.fire("transformend");
    });
    expect(shapes(f)[0]).toMatchObject({ rect: { x: 20, y: 20, width: 130, height: 80 } });
    expect(f.state().history.past).toHaveLength(1);
    expect(f.stage.findOne("#r")!.scaleX()).toBe(1);
  });

  it("moves a selection of several shapes in one step", () => {
    const f = renderCanvas({
      shapes: [
        { ...rect("r"), rect: { x: 20, y: 20, width: 100, height: 60 } },
        { ...mosaic, rect: { x: 200, y: 100, width: 32, height: 32 } },
      ],
    });
    act(() => f.state().select(["r", "m"]));
    f.stageDrag({ x: 40, y: 20 }, { x: 70, y: 40 });
    expect(shapes(f)).toMatchObject([
      { rect: { x: 50, y: 40, width: 100, height: 60 } },
      { rect: { x: 230, y: 120, width: 32, height: 32 } },
    ]);
    expect(f.state().history.past).toHaveLength(1);
  });

  it("resizes a rectangle and a mosaic together in one step", () => {
    const f = renderCanvas({
      shapes: [
        { ...rect("r"), rect: { x: 20, y: 20, width: 100, height: 60 } },
        { ...mosaic, rect: { x: 200, y: 100, width: 32, height: 32 } },
      ],
    });
    act(() => f.state().select(["r", "m"]));
    const transformer = f.stage.findOne<Konva.Transformer>("Transformer")!;
    // As Konva ends a transform: the transformer, then each node, with the same event.
    const evt = new MouseEvent("mouseup");
    act(() => {
      for (const node of transformer.nodes()) node.scale({ x: 2, y: 2 });
      transformer.fire("transformend", { evt });
      for (const node of transformer.nodes()) node.fire("transformend", { evt });
    });
    expect(shapes(f)).toMatchObject([
      { rect: { x: 20, y: 20, width: 200, height: 120 } },
      { rect: { x: 200, y: 100, width: 64, height: 64 } },
    ]);
    expect(f.state().history.past).toHaveLength(1);
  });

  it("moves an arrow's end with its handle", () => {
    const f = renderCanvas({ shapes: [arrow("a")] });
    act(() => f.state().select(["a"]));
    f.stageDrag({ x: 100, y: 50 }, { x: 150, y: 90 });
    expect(shapes(f)[0]).toMatchObject({ start: { x: 10, y: 10 }, end: { x: 150, y: 90 } });
  });

  it("moves mosaics in whole pixels and pixelates the new place", () => {
    const f = renderCanvas({
      shapes: [{ ...mosaic, rect: { x: 40, y: 40, width: 32, height: 32 } }],
    });
    act(() => f.state().select(["m"]));
    f.stageDrag({ x: 50, y: 50 }, { x: 60.4, y: 55.6 });
    expect(shapes(f)[0]).toMatchObject({ rect: { x: 50, y: 46, width: 32, height: 32 } });
    const node = f.stage.findOne<Konva.Image>("#m")!;
    expect([node.x(), node.y()]).toEqual([50, 46]);
  });

  it("starts editing a text on double click", () => {
    const f = renderCanvas({ shapes: [{ ...text("t", "AB"), position: { x: 10, y: 10 } }] });
    // Konva makes a double click of two clicks on the same shape.
    for (let i = 0; i < 2; i++) {
      f.down({ x: 14, y: 14 });
      f.up({ x: 14, y: 14 });
    }
    expect(f.state().textEdit).toMatchObject({ shapeId: "t", draft: "AB" });
    // The text being edited is hidden from the canvas.
    expect(f.stage.findOne("#t")).toBeUndefined();
  });

  it("moves the crop draft with its frame", () => {
    const f = renderCanvas();
    act(() => f.state().setTool("crop"));
    act(() => f.state().updateCropDraft({ x: 100, y: 100, width: 100, height: 50 }));
    f.stageDrag({ x: 150, y: 120 }, { x: 170.4, y: 130 });
    expect(f.state().cropDraft).toEqual({ x: 120, y: 110, width: 100, height: 50 });
  });

  it("zooms with Ctrl+wheel around the pointer and pans with the wheel", () => {
    const f = renderCanvas();
    f.wheel({ deltaY: -100, ctrlKey: true });
    expect(f.state().view).toMatchObject({ zoom: 1.25, mode: "manual" });
    // The image point under the pointer (image 0, 0) stays put.
    expect(f.state().view.pan).toEqual({ x: 200, y: 150 });

    f.wheel({ deltaY: 30, deltaX: 10 });
    expect(f.state().view.pan).toEqual({ x: 190, y: 120 });
    f.wheel({ deltaY: 30, shiftKey: true });
    expect(f.state().view.pan).toEqual({ x: 160, y: 120 });
  });
});
