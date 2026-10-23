// The editing surface: a stage fixed to its container, showing the image at the store's zoom
// and pan. Layers, bottom up: the original image, the mosaics, the other shapes with the
// selection handles, and the crop mask. Outside cropping, the view is clipped to the crop.
import type Konva from "konva";
import { useEffect, useRef, useState } from "react";
import { Group, Image, Layer, Stage } from "react-konva";
import { useShallow } from "zustand/react/shallow";
import type { MosaicShape, Point, Shape } from "../model/document";
import { addShape } from "../model/operations";
import { currentDoc } from "../store/editor-store";
import { useEditor, useEditorContext } from "../store/editor-context";
import { stageScale } from "../store/view";
import { CropOverlay } from "./CropOverlay";
import { SelectionHandles } from "./SelectionHandles";
import { MosaicNode, SHAPE_NODE, ShapeNode, ShapeView } from "./ShapeNode";
import {
  type DragResult,
  type DragToolId,
  dragResult,
  insideImage,
  isDragTool,
  markerAt,
} from "./tools";

/** Pixels a wheel notch pans by when the device reports lines instead of pixels. */
const LINE_HEIGHT = 40;

interface Drag {
  tool: DragToolId;
  start: Point;
  id: string;
}

interface Pan {
  pointer: Point;
  pan: Point;
}

function isTyping(target: EventTarget | null): boolean {
  return (
    target instanceof HTMLElement &&
    (target.isContentEditable || ["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName))
  );
}

/** Follows the container's size and the device pixel ratio into the store. */
function useViewport(container: React.RefObject<HTMLDivElement | null>) {
  const { store } = useEditorContext();
  useEffect(() => {
    const element = container.current;
    if (element === null) return;
    const report = () =>
      store
        .getState()
        .setViewport(
          { width: element.clientWidth, height: element.clientHeight },
          window.devicePixelRatio || 1,
        );
    report();
    const observer = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(report);
    observer?.observe(element);
    // Moving to a monitor with another scale changes the ratio without resizing.
    let media: MediaQueryList | null = null;
    const watchRatio = () => {
      media?.removeEventListener("change", onRatio);
      media = window.matchMedia?.(`(resolution: ${window.devicePixelRatio || 1}dppx)`) ?? null;
      media?.addEventListener("change", onRatio);
    };
    function onRatio() {
      report();
      watchRatio();
    }
    watchRatio();
    return () => {
      observer?.disconnect();
      media?.removeEventListener("change", onRatio);
    };
  }, [container, store]);
}

/** Whether Space is held (outside text inputs), for panning by dragging. */
function useSpaceHeld(): boolean {
  const [held, setHeld] = useState(false);
  useEffect(() => {
    const down = (e: KeyboardEvent) => {
      if (e.code === "Space" && !isTyping(e.target)) {
        e.preventDefault();
        setHeld(true);
      }
    };
    const up = (e: KeyboardEvent) => {
      if (e.code === "Space") setHeld(false);
    };
    const reset = () => setHeld(false);
    window.addEventListener("keydown", down);
    window.addEventListener("keyup", up);
    window.addEventListener("blur", reset);
    return () => {
      window.removeEventListener("keydown", down);
      window.removeEventListener("keyup", up);
      window.removeEventListener("blur", reset);
    };
  }, []);
  return held;
}

export function EditorCanvas() {
  const { store, source, newId } = useEditorContext();
  const container = useRef<HTMLDivElement>(null);
  const stage = useRef<Konva.Stage>(null);
  const [drag, setDrag] = useState<Drag | null>(null);
  const [preview, setPreview] = useState<DragResult | null>(null);
  const [panning, setPanning] = useState<Pan | null>(null);
  const spaceHeld = useSpaceHeld();
  useViewport(container);

  const { view, viewport, dpr, tool, cropping } = useEditor(
    useShallow((s) => ({
      view: s.view,
      viewport: s.viewport,
      dpr: s.devicePixelRatio,
      tool: s.tool,
      cropping: s.cropDraft !== null,
    })),
  );
  const doc = useEditor((s) => currentDoc(s));
  const scale = stageScale(view.zoom, dpr);
  const clip = cropping ? undefined : (doc.crop ?? undefined);
  const mosaics = doc.shapes.filter((s): s is MosaicShape => s.kind === "mosaic");
  const others = doc.shapes.filter((s): s is Exclude<Shape, MosaicShape> => s.kind !== "mosaic");

  /** The image point under the pointer of `evt`. */
  const imagePoint = (evt: MouseEvent): Point | null => {
    const node = stage.current;
    if (node === null) return null;
    node.setPointersPositions(evt);
    return node.getRelativePointerPosition();
  };

  // A drag continues outside the stage, until the button is released anywhere.
  useEffect(() => {
    if (drag === null) return;
    const toolContext = () => {
      const state = store.getState();
      return { doc: currentDoc(state), style: state.style, newId };
    };
    const move = (evt: MouseEvent) => {
      const at = imagePoint(evt);
      if (at === null) return;
      const result = dragResult(drag.tool, drag.start, at, evt.shiftKey, toolContext(), drag.id);
      if (result?.kind === "crop") store.getState().updateCropDraft(result.rect);
      else setPreview(result);
    };
    const up = (evt: MouseEvent) => {
      const at = imagePoint(evt);
      const state = store.getState();
      const result =
        at === null
          ? null
          : dragResult(drag.tool, drag.start, at, evt.shiftKey, toolContext(), drag.id);
      if (result?.kind === "shape") state.commitDoc(addShape(currentDoc(state), result.shape));
      if (result?.kind === "crop") state.updateCropDraft(result.rect);
      setDrag(null);
      setPreview(null);
    };
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
    return () => {
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
    };
    // `imagePoint` only reads refs.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [drag, store, newId]);

  useEffect(() => {
    if (panning === null) return;
    const move = (evt: MouseEvent) =>
      store.getState().setPan({
        x: panning.pan.x + evt.clientX - panning.pointer.x,
        y: panning.pan.y + evt.clientY - panning.pointer.y,
      });
    const up = () => setPanning(null);
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
    return () => {
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
    };
  }, [panning, store]);

  const onMouseDown = (e: Konva.KonvaEventObject<MouseEvent>) => {
    const evt = e.evt;
    if (evt.button === 1 || (evt.button === 0 && spaceHeld)) {
      evt.preventDefault();
      setPanning({ pointer: { x: evt.clientX, y: evt.clientY }, pan: store.getState().view.pan });
      return;
    }
    if (evt.button !== 0) return;
    const state = store.getState();
    const at = imagePoint(evt);
    if (at === null) return;
    const onShape = e.target.hasName(SHAPE_NODE);
    if (state.tool === "select") {
      if (!onShape) state.select([]);
      return;
    }
    if (state.textEdit !== null) {
      // The first click outside the text box only commits it.
      state.commitTextEdit();
      return;
    }
    if (isDragTool(state.tool)) {
      if (state.tool !== "crop" && !insideImage(at, currentDoc(state))) return;
      setDrag({ tool: state.tool, start: at, id: newId() });
      return;
    }
    if (!insideImage(at, currentDoc(state))) return;
    if (state.tool === "text") {
      state.beginTextEdit({ position: at });
    } else if (state.tool === "marker") {
      const current = currentDoc(state);
      state.commitDoc(addShape(current, markerAt(at, { doc: current, style: state.style, newId })));
    }
  };

  const onWheel = (e: Konva.KonvaEventObject<WheelEvent>) => {
    const evt = e.evt;
    evt.preventDefault();
    const state = store.getState();
    if (evt.ctrlKey) {
      const anchor = stage.current?.getPointerPosition() ?? undefined;
      if (evt.deltaY < 0) state.zoomIn(anchor);
      else if (evt.deltaY > 0) state.zoomOut(anchor);
      return;
    }
    const unit = evt.deltaMode === 1 ? LINE_HEIGHT : 1;
    const dx = (evt.shiftKey ? evt.deltaY : evt.deltaX) * unit;
    const dy = (evt.shiftKey ? 0 : evt.deltaY) * unit;
    const { pan } = state.view;
    state.setPan({ x: pan.x - dx, y: pan.y - dy });
  };

  let cursor = "default";
  if (panning !== null) cursor = "grabbing";
  else if (spaceHeld) cursor = "grab";
  else if (tool === "text") cursor = "text";
  else if (tool !== "select") cursor = "crosshair";

  return (
    <div ref={container} className="editor-canvas" style={{ cursor }} data-testid="editor-canvas">
      <Stage
        ref={stage}
        width={viewport.width}
        height={viewport.height}
        scaleX={scale}
        scaleY={scale}
        x={view.pan.x}
        y={view.pan.y}
        onMouseDown={onMouseDown}
        onWheel={onWheel}
      >
        <Layer>
          <Group clip={clip}>
            <Image image={source.image} x={0} y={0} />
          </Group>
        </Layer>
        <Layer imageSmoothingEnabled={false}>
          <Group clip={clip}>
            {mosaics.map((shape) => (
              <MosaicNode key={shape.id} shape={shape} />
            ))}
            {preview?.kind === "shape" && preview.shape.kind === "mosaic" ? (
              <MosaicNode shape={preview.shape} preview />
            ) : null}
          </Group>
        </Layer>
        <Layer>
          <Group clip={clip}>
            {others.map((shape) => (
              <ShapeNode key={shape.id} shape={shape} />
            ))}
            {preview?.kind === "shape" && preview.shape.kind !== "mosaic" ? (
              <ShapeView shape={preview.shape} />
            ) : null}
          </Group>
          <SelectionHandles scale={scale} />
        </Layer>
        {cropping ? (
          <Layer>
            <CropOverlay scale={scale} />
          </Layer>
        ) : null}
      </Stage>
    </div>
  );
}
