// From the overlay's CSS pixels to its monitor's local physical pixels. An overlay covers
// exactly its monitor, so CSS px × scale is the physical offset from the monitor's corner.
import type { PhysicalPoint, PhysicalRect } from "@/shared/ipc/screen-capture";

/** Monitor size in physical pixels. */
export interface PhysicalSize {
  width: number;
  height: number;
}

/** One CSS coordinate as physical pixels, rounded to the nearest pixel edge. */
export function cssToPhysical(css: number, scale: number): number {
  return Math.round(css * scale);
}

/**
 * A pointer position as a local physical point kept on the monitor. The far edges are
 * allowed: a drag to the edge ends there.
 */
export function pointerToPhysical(
  clientX: number,
  clientY: number,
  scale: number,
  size: PhysicalSize,
): PhysicalPoint {
  return {
    x: clamp(cssToPhysical(clientX, scale), 0, size.width),
    y: clamp(cssToPhysical(clientY, scale), 0, size.height),
  };
}

/** The rectangle between two corners, in either order. */
export function rectBetween(a: PhysicalPoint, b: PhysicalPoint): PhysicalRect {
  const x = Math.min(a.x, b.x);
  const y = Math.min(a.y, b.y);
  return { x, y, width: Math.abs(a.x - b.x), height: Math.abs(a.y - b.y) };
}

/** The part of `rect` inside `bounds`, or `null`. Both in the same coordinates. */
export function intersect(rect: PhysicalRect, bounds: PhysicalRect): PhysicalRect | null {
  const left = Math.max(rect.x, bounds.x);
  const top = Math.max(rect.y, bounds.y);
  const right = Math.min(rect.x + rect.width, bounds.x + bounds.width);
  const bottom = Math.min(rect.y + rect.height, bounds.y + bounds.height);
  if (right <= left || bottom <= top) return null;
  return { x: left, y: top, width: right - left, height: bottom - top };
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(Math.max(value, min), max);
}
