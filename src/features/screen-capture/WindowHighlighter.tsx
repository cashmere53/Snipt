// Window mode: the overlay lies over the live screen; Rust tells which window is under the
// pointer and every overlay frames its part of it. A click captures that window.
import { type PointerEvent, useMemo, useRef } from "react";
import type { OverlayActivate, PhysicalRect } from "@/shared/ipc/screen-capture";
import { intersect, pointerToPhysical } from "./coords";
import { frameThrottle, type NextFrame } from "./frame-throttle";
import type { OverlayIpc } from "./services/overlay-ipc";

interface WindowHighlighterProps {
  session: OverlayActivate;
  /** Global physical pixels. */
  highlight: PhysicalRect | null;
  ipc: OverlayIpc;
  nextFrame: NextFrame;
}

export function WindowHighlighter({ session, highlight, ipc, nextFrame }: WindowHighlighterProps) {
  const { monitor } = session;
  const surface = useRef<HTMLDivElement>(null);
  const sendPointer = useMemo(
    () =>
      frameThrottle(nextFrame, ({ x, y }: { x: number; y: number }) => {
        void ipc.pointer(session.sessionId, monitor.index, x, y);
      }),
    [ipc, nextFrame, session.sessionId, monitor.index],
  );

  function onPointerMove(event: PointerEvent<HTMLDivElement>) {
    sendPointer(
      pointerToPhysical(event.clientX, event.clientY, monitor.scale, {
        width: monitor.rect.width,
        height: monitor.rect.height,
      }),
    );
  }

  function onClick() {
    const element = surface.current;
    if (!element) return;
    // Windows before 10 2004 cannot leave the cursor out of a window capture.
    element.style.cursor = "none";
    void ipc.commitWindow(session.sessionId).then((result) => {
      if (!result.ok) element.style.cursor = "";
    });
  }

  const part = highlight ? intersect(highlight, monitor.rect) : null;

  return (
    <div
      ref={surface}
      className="overlay-window"
      data-testid="window-highlighter"
      onPointerMove={onPointerMove}
      onClick={onClick}
    >
      {part && (
        <div
          className="overlay-highlight"
          data-testid="highlight"
          style={{
            left: (part.x - monitor.rect.x) / monitor.scale,
            top: (part.y - monitor.rect.y) / monitor.scale,
            width: part.width / monitor.scale,
            height: part.height / monitor.scale,
          }}
        />
      )}
    </div>
  );
}
