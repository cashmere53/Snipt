// The enlarged pixels around the pointer, read from the frozen screen in Rust, with the
// pointer's global physical coordinates.
import { useEffect, useRef } from "react";
import type { OverlayMonitor, PhysicalPoint } from "@/shared/ipc/screen-capture";
import type { NextFrame } from "./frame-throttle";
import type { OverlayIpc } from "./services/overlay-ipc";

export const MAGNIFIER_RADIUS = 8;
export const MAGNIFIER_ZOOM = 8;
const SIDE = 2 * MAGNIFIER_RADIUS + 1;
/** CSS px between the pointer and the magnifier. */
const OFFSET = 20;

interface MagnifierProps {
  sessionId: number;
  monitor: OverlayMonitor;
  /** Local physical pixels. */
  point: PhysicalPoint;
  ipc: OverlayIpc;
  nextFrame: NextFrame;
}

export function Magnifier({ sessionId, monitor, point, ipc, nextFrame }: MagnifierProps) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const latest = useRef(point);
  const pending = useRef(false);

  // At most one request per animation frame, for the latest point.
  useEffect(() => {
    latest.current = point;
    if (pending.current) return;
    pending.current = true;
    nextFrame(() => {
      pending.current = false;
      const { x, y } = latest.current;
      void ipc.magnifier(sessionId, monitor.index, x, y, MAGNIFIER_RADIUS).then((result) => {
        if (result.ok && canvas.current) draw(canvas.current, result.value);
      });
    });
  }, [ipc, nextFrame, point, sessionId, monitor.index]);

  const size = SIDE * MAGNIFIER_ZOOM;
  const viewWidth = monitor.rect.width / monitor.scale;
  const viewHeight = monitor.rect.height / monitor.scale;
  let left = point.x / monitor.scale + OFFSET;
  let top = point.y / monitor.scale + OFFSET;
  // Keep it on screen: flip to the other side of the pointer near the far edges.
  if (left + size > viewWidth) left -= size + 2 * OFFSET;
  if (top + size + 24 > viewHeight) top -= size + 24 + 2 * OFFSET;

  return (
    <div className="overlay-magnifier" style={{ left, top }} data-testid="magnifier">
      <canvas ref={canvas} width={size} height={size} />
      <div className="overlay-coords">
        X: {monitor.rect.x + point.x} Y: {monitor.rect.y + point.y}
      </div>
    </div>
  );
}

function draw(target: HTMLCanvasElement, rgba: Uint8Array) {
  if (rgba.length !== SIDE * SIDE * 4) return;
  const patch = document.createElement("canvas");
  patch.width = SIDE;
  patch.height = SIDE;
  const patchContext = patch.getContext("2d");
  const context = target.getContext("2d");
  if (!patchContext || !context) return;
  const pixels = patchContext.createImageData(SIDE, SIDE);
  pixels.data.set(rgba);
  patchContext.putImageData(pixels, 0, 0);
  context.imageSmoothingEnabled = false;
  context.clearRect(0, 0, target.width, target.height);
  context.drawImage(patch, 0, 0, target.width, target.height);
  // The pixel under the pointer.
  context.strokeStyle = "rgb(255 255 255 / 80%)";
  context.strokeRect(
    MAGNIFIER_RADIUS * MAGNIFIER_ZOOM + 0.5,
    MAGNIFIER_RADIUS * MAGNIFIER_ZOOM + 0.5,
    MAGNIFIER_ZOOM - 1,
    MAGNIFIER_ZOOM - 1,
  );
}
