// Zoom and pan of the editor view. `zoom` is physical screen pixels per image pixel, so 100%
// shows the image 1:1 on the screen whatever the DPI; Konva's stage scale is `zoom / dpr`
// (the stage works in CSS pixels). `pan` is the stage position in CSS pixels.
import type { ImageSize, Point, Rect } from "../model/document";

export interface ViewState {
  zoom: number;
  pan: Point;
  /** `fit` follows the viewport and the shown area; any zoom or pan makes it `manual`. */
  mode: "fit" | "manual";
}

/** The view area in CSS pixels. */
export type Viewport = ImageSize;

export const MIN_ZOOM = 0.1;
export const MAX_ZOOM = 8;
export const ZOOM_STEPS: readonly number[] = [
  0.1, 0.25, 0.33, 0.5, 0.67, 0.75, 1, 1.25, 1.5, 2, 3, 4, 6, 8,
];

const EPSILON = 1e-6;

export const clampZoom = (zoom: number) => Math.min(Math.max(zoom, MIN_ZOOM), MAX_ZOOM);

export const stageScale = (zoom: number, dpr: number) => zoom / dpr;

export function nextZoomStep(zoom: number): number {
  return ZOOM_STEPS.find((step) => step > zoom + EPSILON) ?? MAX_ZOOM;
}

export function previousZoomStep(zoom: number): number {
  return ZOOM_STEPS.findLast((step) => step < zoom - EPSILON) ?? MIN_ZOOM;
}

const isEmpty = (viewport: Viewport) => viewport.width <= 0 || viewport.height <= 0;

/** Shows all of `target` centered: shrinks large areas to fit, never enlarges small ones. */
export function fitView(target: Rect, viewport: Viewport, dpr: number): ViewState {
  if (isEmpty(viewport)) {
    return { zoom: 1, pan: { x: -target.x / dpr, y: -target.y / dpr }, mode: "fit" };
  }
  const zoom = clampZoom(
    Math.min(1, (viewport.width * dpr) / target.width, (viewport.height * dpr) / target.height),
  );
  return { zoom, pan: centeredPan(target, viewport, stageScale(zoom, dpr)), mode: "fit" };
}

function centeredPan(target: Rect, viewport: Viewport, scale: number): Point {
  return {
    x: (viewport.width - target.width * scale) / 2 - target.x * scale,
    y: (viewport.height - target.height * scale) / 2 - target.y * scale,
  };
}

/** Changes the zoom keeping the image point under `anchor` (CSS pixels in the view) still. */
export function zoomAround(view: ViewState, zoom: number, anchor: Point, dpr: number): ViewState {
  const next = clampZoom(zoom);
  const before = stageScale(view.zoom, dpr);
  const after = stageScale(next, dpr);
  const imageX = (anchor.x - view.pan.x) / before;
  const imageY = (anchor.y - view.pan.y) / before;
  return {
    zoom: next,
    pan: { x: anchor.x - imageX * after, y: anchor.y - imageY * after },
    mode: "manual",
  };
}

export const viewportCenter = (viewport: Viewport): Point => ({
  x: viewport.width / 2,
  y: viewport.height / 2,
});
