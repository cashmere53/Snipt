// Moving and resizing shapes from what Konva reports at the end of a drag or a transform:
// the intermediate state lives in the nodes, and these turn the final state into one patch.
import type { ImageSize, Point, Rect, Shape } from "../model/document";
import { toIntegerRect } from "../model/geometry";

/** `shape` moved by `delta` image pixels. */
export function moveShape<S extends Shape>(shape: S, delta: Point): S {
  const by = (p: Point): Point => ({ x: p.x + delta.x, y: p.y + delta.y });
  switch (shape.kind) {
    case "arrow":
      return { ...shape, start: by(shape.start), end: by(shape.end) };
    case "rect":
    case "mosaic":
      return { ...shape, rect: { ...shape.rect, ...by(shape.rect) } };
    case "text":
      return { ...shape, position: by(shape.position) };
    case "marker":
      return { ...shape, center: by(shape.center) };
  }
}

/** The attributes of a transformed rectangle node: its scale is folded into its size. */
export interface NodeBox {
  x: number;
  y: number;
  width: number;
  height: number;
  scaleX: number;
  scaleY: number;
}

export function bakeScale(box: NodeBox): Rect {
  // A flip (negative scale) moves the origin to the other side.
  const width = box.width * box.scaleX;
  const height = box.height * box.scaleY;
  return {
    x: width < 0 ? box.x + width : box.x,
    y: height < 0 ? box.y + height : box.y,
    width: Math.abs(width),
    height: Math.abs(height),
  };
}

/** A mosaic or crop keeps whole pixels inside the image; `null` when nothing is left. */
export function bakeIntegerRect(box: NodeBox, image: ImageSize): Rect | null {
  return toIntegerRect(bakeScale(box), image);
}
