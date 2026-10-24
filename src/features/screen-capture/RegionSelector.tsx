// Region mode: dims the frozen screen (shown by the native backdrop underneath) outside the
// selection, which a drag makes within this monitor. Everything is in physical pixels.
import { type PointerEvent, useEffect, useRef, useState } from "react";
import type { OverlayActivate, PhysicalPoint, PhysicalRect } from "@/shared/ipc/screen-capture";
import { pointerToPhysical, rectBetween } from "./coords";
import type { NextFrame } from "./frame-throttle";
import { Magnifier } from "./Magnifier";
import type { OverlayIpc } from "./services/overlay-ipc";

const DIM = "rgb(0 0 0 / 40%)";
const OUTLINE = "rgb(255 255 255 / 90%)";

interface RegionSelectorProps {
  session: OverlayActivate;
  ipc: OverlayIpc;
  nextFrame: NextFrame;
}

export function RegionSelector({ session, ipc, nextFrame }: RegionSelectorProps) {
  const { monitor } = session;
  const size = { width: monitor.rect.width, height: monitor.rect.height };
  const canvas = useRef<HTMLCanvasElement>(null);
  /** Where the drag started; a ref, so that every event sees the latest. */
  const anchor = useRef<PhysicalPoint | null>(null);
  const [selection, setSelection] = useState<PhysicalRect | null>(null);
  const [pointer, setPointer] = useState<PhysicalPoint | null>(null);

  useEffect(() => {
    const context = canvas.current?.getContext("2d");
    if (!context) return;
    context.clearRect(0, 0, size.width, size.height);
    context.fillStyle = DIM;
    context.fillRect(0, 0, size.width, size.height);
    if (selection) {
      context.clearRect(selection.x, selection.y, selection.width, selection.height);
      context.strokeStyle = OUTLINE;
      context.lineWidth = 1;
      context.strokeRect(
        selection.x + 0.5,
        selection.y + 0.5,
        selection.width - 1,
        selection.height - 1,
      );
    }
  }, [selection, size.width, size.height]);

  const at = (event: PointerEvent) =>
    pointerToPhysical(event.clientX, event.clientY, monitor.scale, size);

  function onPointerDown(event: PointerEvent<HTMLDivElement>) {
    if (event.button !== 0) return;
    event.currentTarget.setPointerCapture?.(event.pointerId);
    const point = at(event);
    anchor.current = point;
    setSelection(rectBetween(point, point));
    setPointer(point);
  }

  function onPointerMove(event: PointerEvent<HTMLDivElement>) {
    const point = at(event);
    setPointer(point);
    if (anchor.current) setSelection(rectBetween(anchor.current, point));
  }

  function onPointerUp(event: PointerEvent<HTMLDivElement>) {
    if (!anchor.current || event.button !== 0) return;
    const rect = rectBetween(anchor.current, at(event));
    anchor.current = null;
    if (rect.width < 1 || rect.height < 1) {
      // A click, not a drag: keep selecting.
      setSelection(null);
      return;
    }
    setSelection(rect);
    void ipc.commitRegion(session.sessionId, monitor.index, rect).then((result) => {
      // Too small for Rust as well: keep selecting. Otherwise Rust ends the session.
      if (!result.ok && result.error.kind === "tooSmall") setSelection(null);
    });
  }

  return (
    <div
      className="overlay-region"
      data-testid="region-selector"
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
    >
      <canvas
        ref={canvas}
        width={size.width}
        height={size.height}
        style={{ width: size.width / monitor.scale, height: size.height / monitor.scale }}
      />
      {selection && selection.width > 0 && selection.height > 0 && (
        <div
          className="overlay-size"
          style={{
            left: selection.x / monitor.scale,
            top: Math.max(0, selection.y / monitor.scale - 22),
          }}
        >
          {selection.width} × {selection.height}
        </div>
      )}
      {pointer && (
        <Magnifier
          sessionId={session.sessionId}
          monitor={monitor}
          point={pointer}
          ipc={ipc}
          nextFrame={nextFrame}
        />
      )}
    </div>
  );
}
