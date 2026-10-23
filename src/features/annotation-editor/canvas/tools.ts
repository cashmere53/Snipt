// The drawing tools: what a drag (arrow, rectangle, mosaic, crop) or a click (text, marker)
// makes, in image pixels. Pure, so the canvas only feeds pointer positions in and commits the
// result; the tool stays selected after drawing.
import type {
  AnnotationDocument,
  ArrowShape,
  MarkerShape,
  MosaicShape,
  Point,
  Rect,
  RectShape,
} from "../model/document";
import { constrainSquare, normalizeRect, snapAngle45, toIntegerRect } from "../model/geometry";
import { mosaicBlockSize } from "../model/mosaic";
import { nextMarkerNumber } from "../model/operations";
import type { StyleSettings } from "../model/style";
import type { ToolId } from "../store/editor-store";

export type DragToolId = "arrow" | "rect" | "mosaic" | "crop";
export type ClickToolId = "text" | "marker";

export const DRAG_TOOLS: readonly ToolId[] = ["arrow", "rect", "mosaic", "crop"];
export const isDragTool = (tool: ToolId): tool is DragToolId => DRAG_TOOLS.includes(tool);
export const isClickTool = (tool: ToolId): tool is ClickToolId =>
  tool === "text" || tool === "marker";

/** Shorter drags are clicks: they make nothing. */
export const MIN_DRAG = 3;

export interface ToolContext {
  doc: AnnotationDocument;
  style: StyleSettings;
  newId(): string;
}

export type DragResult =
  | { kind: "shape"; shape: ArrowShape | RectShape | MosaicShape }
  | { kind: "crop"; rect: Rect };

/** A marker's diameter follows the text size, so numbers read like the texts around them. */
export const markerSize = (style: StyleSettings) => Math.round((style.fontSize * 4) / 3);

/**
 * What a drag from `start` to `current` makes (the preview while dragging, the result when
 * released), or `null` when it is too small. Shift snaps arrows to 45° and makes squares.
 * `id` is kept between the preview and the result.
 */
export function dragResult(
  tool: DragToolId,
  start: Point,
  current: Point,
  shift: boolean,
  ctx: ToolContext,
  id: string,
): DragResult | null {
  const { doc, style } = ctx;
  switch (tool) {
    case "arrow": {
      const end = shift ? snapAngle45(start, current) : current;
      if (Math.hypot(end.x - start.x, end.y - start.y) < MIN_DRAG) return null;
      return {
        kind: "shape",
        shape: {
          id,
          kind: "arrow",
          start: { ...start },
          end: { ...end },
          color: style.color,
          strokeWidth: style.strokeWidth,
        },
      };
    }
    case "rect": {
      const rect = shift ? constrainSquare(start, current) : normalizeRect(start, current);
      if (rect.width < MIN_DRAG || rect.height < MIN_DRAG) return null;
      return {
        kind: "shape",
        shape: { id, kind: "rect", rect, color: style.color, strokeWidth: style.strokeWidth },
      };
    }
    case "mosaic": {
      const rect = toIntegerRect(normalizeRect(start, current), doc.image);
      if (rect === null || rect.width < MIN_DRAG || rect.height < MIN_DRAG) return null;
      return {
        kind: "shape",
        shape: { id, kind: "mosaic", rect, blockSize: mosaicBlockSize(doc.image) },
      };
    }
    case "crop": {
      const rect = toIntegerRect(normalizeRect(start, current), doc.image);
      if (rect === null || rect.width < MIN_DRAG || rect.height < MIN_DRAG) return null;
      return { kind: "crop", rect };
    }
  }
}

/** The marker a click at `at` places: the next number, never reusing deleted ones. */
export function markerAt(at: Point, ctx: ToolContext): MarkerShape {
  return {
    id: ctx.newId(),
    kind: "marker",
    center: { ...at },
    number: nextMarkerNumber(ctx.doc),
    color: ctx.style.color,
    size: markerSize(ctx.style),
  };
}

/** Whether `point` is inside the image (clicks outside it make nothing). */
export function insideImage(point: Point, doc: AnnotationDocument): boolean {
  const area = doc.crop ?? { x: 0, y: 0, ...doc.image };
  return (
    point.x >= area.x &&
    point.y >= area.y &&
    point.x <= area.x + area.width &&
    point.y <= area.y + area.height
  );
}
