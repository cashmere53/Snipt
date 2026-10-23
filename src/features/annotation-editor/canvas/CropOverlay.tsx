// Cropping: the whole image is shown, everything outside the crop draft is dimmed, and the
// draft is moved or resized with handles. Confirming and cancelling are the store's.
import type Konva from "konva";
import { useEffect, useRef } from "react";
import { Group, Rect, Transformer } from "react-konva";
import type { Rect as RectArea } from "../model/document";
import { currentDoc } from "../store/editor-store";
import { useEditor, useEditorContext } from "../store/editor-context";
import { bakeIntegerRect } from "./transform";

const DIM = "rgba(0, 0, 0, 0.5)";

/** The four bands of `whole` around `hole`. */
export function bandsAround(hole: RectArea, whole: RectArea): RectArea[] {
  const right = hole.x + hole.width;
  const bottom = hole.y + hole.height;
  return [
    { x: whole.x, y: whole.y, width: whole.width, height: hole.y - whole.y },
    { x: whole.x, y: bottom, width: whole.width, height: whole.y + whole.height - bottom },
    { x: whole.x, y: hole.y, width: hole.x - whole.x, height: hole.height },
    { x: right, y: hole.y, width: whole.x + whole.width - right, height: hole.height },
  ].filter((band) => band.width > 0 && band.height > 0);
}

export function CropOverlay({ scale }: { scale: number }) {
  const { store } = useEditorContext();
  const draft = useEditor((s) => s.cropDraft);
  const image = useEditor((s) => currentDoc(s).image);
  const frame = useRef<Konva.Rect>(null);
  const transformer = useRef<Konva.Transformer>(null);

  // Mounted only while cropping, so the frame exists from the start.
  useEffect(() => {
    if (frame.current && transformer.current) {
      transformer.current.nodes([frame.current]);
      transformer.current.getLayer()?.batchDraw();
    }
  }, []);

  if (draft === null) return null;
  const whole = { x: 0, y: 0, ...image };

  const commit = (node: Konva.Node) => {
    const rect = bakeIntegerRect(
      {
        x: node.x(),
        y: node.y(),
        width: node.width(),
        height: node.height(),
        scaleX: node.scaleX(),
        scaleY: node.scaleY(),
      },
      currentDoc(store.getState()).image,
    );
    node.scale({ x: 1, y: 1 });
    if (rect !== null) {
      node.setAttrs(rect);
      store.getState().updateCropDraft(rect);
    } else {
      node.setAttrs(draft);
    }
  };

  return (
    <Group>
      {bandsAround(draft, whole).map((band, i) => (
        <Rect key={i} {...band} fill={DIM} listening={false} />
      ))}
      <Rect
        ref={frame}
        name="crop-frame"
        {...draft}
        stroke="#FFFFFF"
        strokeWidth={1 / scale}
        dash={[6 / scale, 4 / scale]}
        draggable
        onMouseDown={(e) => (e.cancelBubble = true)}
        onDragEnd={(e) => commit(e.target)}
        onTransformEnd={(e) => commit(e.target)}
      />
      <Transformer
        ref={transformer}
        rotateEnabled={false}
        keepRatio={false}
        flipEnabled={false}
        ignoreStroke
        borderEnabled={false}
        anchorStroke="#1E88E5"
        anchorSize={8}
      />
    </Group>
  );
}
