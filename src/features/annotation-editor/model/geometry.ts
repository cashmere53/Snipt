// Geometry helpers for drawing: rectangles from drags, Shift constraints, integer clamping.
import type { ImageSize, Point, Rect } from "./document";

const STEP = Math.PI / 4;

/** The rectangle spanned by two corners, in any order. */
export function normalizeRect(a: Point, b: Point): Rect {
  return {
    x: Math.min(a.x, b.x),
    y: Math.min(a.y, b.y),
    width: Math.abs(b.x - a.x),
    height: Math.abs(b.y - a.y),
  };
}

/**
 * The end point at the nearest multiple of 45 degrees from `start`, as far along that direction
 * as `current` reaches (Shift while drawing an arrow).
 */
export function snapAngle45(start: Point, current: Point): Point {
  const dx = current.x - start.x;
  const dy = current.y - start.y;
  if (dx === 0 && dy === 0) {
    return { ...start };
  }
  const angle = Math.round(Math.atan2(dy, dx) / STEP) * STEP;
  const cos = Math.round(Math.cos(angle) * 1e12) / 1e12;
  const sin = Math.round(Math.sin(angle) * 1e12) / 1e12;
  const length = dx * cos + dy * sin;
  return { x: start.x + length * cos, y: start.y + length * sin };
}

/** A square from `start` towards `current`, as large as the longer side (Shift for rect). */
export function constrainSquare(start: Point, current: Point): Rect {
  const dx = current.x - start.x;
  const dy = current.y - start.y;
  const side = Math.max(Math.abs(dx), Math.abs(dy));
  return {
    x: dx < 0 ? start.x - side : start.x,
    y: dy < 0 ? start.y - side : start.y,
    width: side,
    height: side,
  };
}

const clamp = (value: number, max: number) => Math.min(Math.max(value, 0), max);

/**
 * Rounds the edges to whole pixels and clamps them into the image; `null` when nothing of
 * the rectangle is left.
 */
export function toIntegerRect(rect: Rect, bounds: ImageSize): Rect | null {
  const left = clamp(Math.round(Math.min(rect.x, rect.x + rect.width)), bounds.width);
  const right = clamp(Math.round(Math.max(rect.x, rect.x + rect.width)), bounds.width);
  const top = clamp(Math.round(Math.min(rect.y, rect.y + rect.height)), bounds.height);
  const bottom = clamp(Math.round(Math.max(rect.y, rect.y + rect.height)), bounds.height);
  if (right <= left || bottom <= top) {
    return null;
  }
  return { x: left, y: top, width: right - left, height: bottom - top };
}
