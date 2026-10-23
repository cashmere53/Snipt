// Handles of the selection: a transformer without rotation for rectangles and mosaics (Shift
// keeps the ratio), a frame for texts and markers (they only move), and an anchor at each end
// of an arrow. Each change is committed once, when the gesture ends.
import type Konva from "konva";
import { useEffect, useRef } from "react";
import { Circle, Transformer } from "react-konva";
import { useShallow } from "zustand/react/shallow";
import type { ArrowShape, Point, RectShape } from "../model/document";
import { updateShape } from "../model/operations";
import { currentDoc } from "../store/editor-store";
import { useEditor, useEditorContext } from "../store/editor-context";
import { bakeScale } from "./transform";

const HANDLE_COLOR = "#1E88E5";

function ArrowHandles({ arrow, scale }: { arrow: ArrowShape; scale: number }) {
  const { store } = useEditorContext();
  const radius = 6 / scale;

  const move = (which: "start" | "end", node: Konva.Node) => {
    const line = node.getStage()?.findOne<Konva.Arrow>(`#${arrow.id}`);
    const at: Point = node.position();
    const start = which === "start" ? at : arrow.start;
    const end = which === "end" ? at : arrow.end;
    line?.points([start.x, start.y, end.x, end.y]);
  };

  const commit = (which: "start" | "end", node: Konva.Node) => {
    const state = store.getState();
    state.commitDoc(
      updateShape<ArrowShape>(currentDoc(state), arrow.id, { [which]: node.position() }),
    );
  };

  return (["start", "end"] as const).map((which) => (
    <Circle
      key={which}
      name="handle"
      x={arrow[which].x}
      y={arrow[which].y}
      radius={radius}
      fill="#FFFFFF"
      stroke={HANDLE_COLOR}
      strokeWidth={1.5 / scale}
      draggable
      onMouseDown={(e) => (e.cancelBubble = true)}
      onDragMove={(e) => move(which, e.target)}
      onDragEnd={(e) => commit(which, e.target)}
    />
  ));
}

export function SelectionHandles({ scale }: { scale: number }) {
  const { store } = useEditorContext();
  const transformer = useRef<Konva.Transformer>(null);
  const tool = useEditor((s) => s.tool);
  const selected = useEditor(
    useShallow((s) => {
      const ids = new Set(s.selection);
      return currentDoc(s).shapes.filter((shape) => ids.has(shape.id));
    }),
  );
  const editingText = useEditor((s) => s.textEdit?.shapeId ?? null);
  const active = tool === "select" ? selected.filter((shape) => shape.id !== editingText) : [];
  const boxed = active.filter((shape) => shape.kind !== "arrow");
  const resizable = boxed.every((shape) => shape.kind === "rect" || shape.kind === "mosaic");
  const ids = boxed.map((shape) => shape.id).join(" ");

  // After every render: a committed change may move the nodes under the transformer.
  useEffect(() => {
    const node = transformer.current;
    const stage = node?.getStage();
    if (!node || !stage) return;
    const nodes = ids === "" ? [] : ids.split(" ").flatMap((id) => stage.findOne(`#${id}`) ?? []);
    node.nodes(nodes);
    node.getLayer()?.batchDraw();
  });

  // Rectangles fold the scale into their size; mosaics settle themselves (`MosaicNode`).
  const onTransformEnd = () => {
    const state = store.getState();
    let doc = currentDoc(state);
    for (const node of transformer.current?.nodes() ?? []) {
      const shape = doc.shapes.find((s) => s.id === node.id());
      if (shape?.kind !== "rect") continue;
      const rect = bakeScale({
        x: node.x(),
        y: node.y(),
        width: node.width(),
        height: node.height(),
        scaleX: node.scaleX(),
        scaleY: node.scaleY(),
      });
      node.scale({ x: 1, y: 1 });
      doc = updateShape<RectShape>(doc, shape.id, { rect });
    }
    state.commitDoc(doc);
  };

  return (
    <>
      <Transformer
        ref={transformer}
        rotateEnabled={false}
        keepRatio={false}
        flipEnabled={false}
        resizeEnabled={resizable}
        ignoreStroke
        borderStroke={HANDLE_COLOR}
        anchorStroke={HANDLE_COLOR}
        anchorSize={8}
        onTransformEnd={onTransformEnd}
      />
      {active.map((shape) =>
        shape.kind === "arrow" ? <ArrowHandles key={shape.id} arrow={shape} scale={scale} /> : null,
      )}
    </>
  );
}
