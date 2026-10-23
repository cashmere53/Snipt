// The view of one shape, from the same `shape-attrs` as the export. In the select tool a
// shape is selected by a press and moved by a drag; the move is committed once, at the end.
import type Konva from "konva";
import { useEffect, useMemo, useRef, useState } from "react";
import { Arrow, Circle, Group, Image, Label, Rect, Tag, Text } from "react-konva";
import type { MosaicShape, Rect as RectArea, Shape } from "../model/document";
import { updateShape } from "../model/operations";
import { mosaicCanvas } from "../services/composite-renderer";
import { currentDoc } from "../store/editor-store";
import { useEditor, useEditorContext } from "../store/editor-context";
import { arrowAttrs, markerAttrs, mosaicAttrs, rectAttrs, textAttrs } from "./shape-attrs";
import { bakeIntegerRect, moveShape, type NodeBox } from "./transform";

type KonvaEvent = Konva.KonvaEventObject<MouseEvent>;

/** Shape node props for selecting and moving; `undefined` for previews. */
interface Interaction {
  id: string;
  name: string;
  draggable: boolean;
  onMouseDown(e: KonvaEvent): void;
  onDragStart(e: Konva.KonvaEventObject<DragEvent>): void;
  onDragEnd(e: Konva.KonvaEventObject<DragEvent>): void;
  onDblClick?(e: KonvaEvent): void;
}

/** The name every shape node carries, so the canvas can tell shapes from the background. */
export const SHAPE_NODE = "shape";

function useInteraction(shape: Shape): Interaction {
  const { store } = useEditorContext();
  const tool = useEditor((s) => s.tool);
  const selected = useEditor((s) => s.selection.includes(shape.id));
  const origin = useRef({ x: 0, y: 0 });

  return {
    id: shape.id,
    name: SHAPE_NODE,
    draggable: tool === "select" && selected,
    onMouseDown: (e) => {
      if (store.getState().tool !== "select" || e.evt.button !== 0) return;
      e.cancelBubble = true;
      const { selection, select } = store.getState();
      if (e.evt.shiftKey) {
        select(
          selection.includes(shape.id)
            ? selection.filter((id) => id !== shape.id)
            : [...selection, shape.id],
        );
      } else if (!selection.includes(shape.id)) {
        select([shape.id]);
      }
    },
    onDragStart: (e) => {
      origin.current = e.target.position();
    },
    onDragEnd: (e) => {
      const node = e.target;
      const delta = { x: node.x() - origin.current.x, y: node.y() - origin.current.y };
      // Back to the props' position: the committed shape moves it.
      node.position(origin.current);
      const state = store.getState();
      const doc = currentDoc(state);
      const current = doc.shapes.find((s) => s.id === shape.id);
      if (current === undefined || (delta.x === 0 && delta.y === 0)) return;
      state.commitDoc(updateShape(doc, shape.id, moveShape(current, delta)));
    },
    onDblClick: (e) => {
      const { tool: now, beginTextEdit } = store.getState();
      if (shape.kind !== "text" || (now !== "select" && now !== "text")) return;
      e.cancelBubble = true;
      beginTextEdit({ shapeId: shape.id });
    },
  };
}

/** Draws a shape other than a mosaic; without `interaction` it is a preview. */
export function ShapeView({
  shape,
  interaction,
}: {
  shape: Exclude<Shape, MosaicShape>;
  interaction?: Interaction;
}) {
  const listening = interaction !== undefined;
  switch (shape.kind) {
    case "arrow":
      return (
        <Arrow
          {...arrowAttrs(shape)}
          {...interaction}
          listening={listening}
          hitStrokeWidth={Math.max(12, shape.strokeWidth)}
        />
      );
    case "rect":
      return (
        <Rect
          {...rectAttrs(shape)}
          {...interaction}
          listening={listening}
          strokeScaleEnabled={false}
          hitStrokeWidth={Math.max(12, shape.strokeWidth)}
        />
      );
    case "text": {
      const attrs = textAttrs(shape);
      return (
        <Label {...attrs.label} {...interaction} listening={listening}>
          <Tag {...attrs.tag} />
          <Text {...attrs.text} />
        </Label>
      );
    }
    case "marker": {
      const attrs = markerAttrs(shape);
      return (
        <Group {...attrs.group} {...interaction} listening={listening}>
          <Circle {...attrs.circle} />
          <Text {...attrs.label} />
        </Group>
      );
    }
  }
}

export function ShapeNode({ shape }: { shape: Exclude<Shape, MosaicShape> }) {
  const interaction = useInteraction(shape);
  const editing = useEditor((s) => s.textEdit?.shapeId === shape.id);
  // The text being typed is shown by the text box instead.
  if (editing) return null;
  return <ShapeView shape={shape} interaction={interaction} />;
}

/** A node's box as `transform` takes it. */
export const nodeBox = (node: Konva.Node): NodeBox => ({
  x: node.x(),
  y: node.y(),
  width: node.width(),
  height: node.height(),
  scaleX: node.scaleX(),
  scaleY: node.scaleY(),
});

/**
 * A mosaic: the original pixels of its rectangle, pixelated. While it is dragged or resized
 * the blocks follow every frame; the result is committed once at the end.
 */
export function MosaicNode({ shape, preview = false }: { shape: MosaicShape; preview?: boolean }) {
  const { store, source } = useEditorContext();
  const interaction = useInteraction(shape);
  const [moving, setMoving] = useState<RectArea | null>(null);
  const frame = useRef(0);
  const rect = moving ?? shape.rect;
  const image = useMemo(
    () => mosaicCanvas(source, rect, shape.blockSize),
    // The rectangle's values, not its identity.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [source, rect.x, rect.y, rect.width, rect.height, shape.blockSize],
  );
  useEffect(() => () => cancelAnimationFrame(frame.current), []);

  /** The node's current rectangle in whole image pixels, with the scale folded in. */
  const settle = (node: Konva.Node): RectArea | null => {
    const next = bakeIntegerRect(nodeBox(node), currentDoc(store.getState()).image);
    node.scale({ x: 1, y: 1 });
    if (next !== null) node.setAttrs(next);
    return next;
  };

  const follow = (node: Konva.Node) => {
    cancelAnimationFrame(frame.current);
    frame.current = requestAnimationFrame(() => setMoving(settle(node)));
  };

  const commit = (node: Konva.Node) => {
    cancelAnimationFrame(frame.current);
    const next = settle(node);
    setMoving(null);
    const state = store.getState();
    if (next !== null) {
      state.commitDoc(updateShape<MosaicShape>(currentDoc(state), shape.id, { rect: next }));
    }
  };

  if (preview) {
    return <Image {...mosaicAttrs(shape)} image={image} listening={false} />;
  }
  return (
    <Image
      {...mosaicAttrs({ ...shape, rect })}
      image={image}
      {...interaction}
      onDragMove={(e) => follow(e.target)}
      onDragEnd={(e) => commit(e.target)}
      onTransform={(e) => follow(e.target)}
      onTransformEnd={(e) => commit(e.target)}
    />
  );
}
