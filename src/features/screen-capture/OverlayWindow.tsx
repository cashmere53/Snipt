// One overlay window per monitor. It waits hidden; Rust shows it for a capture session and
// sends `overlay://activate`, then hides it again after `overlay://deactivate`.
import { useEffect, useMemo, useRef, useState } from "react";
import type { OverlayActivate, PhysicalRect } from "@/shared/ipc/screen-capture";
import type { NextFrame } from "./frame-throttle";
import { RegionSelector } from "./RegionSelector";
import { createOverlayIpc, type OverlayIpc } from "./services/overlay-ipc";
import { type OverlayPort, tauriOverlayPort, type Unlisten } from "./services/overlay-port";
import { WindowHighlighter } from "./WindowHighlighter";
import "./overlay.css";

export interface OverlayWindowDeps {
  ipc: OverlayIpc;
  port: () => OverlayPort;
  nextFrame: NextFrame;
}

function defaultDeps(): OverlayWindowDeps {
  return {
    ipc: createOverlayIpc(),
    port: tauriOverlayPort,
    nextFrame: (callback) => requestAnimationFrame(() => callback()),
  };
}

interface OverlayWindowProps {
  monitorIndex: number;
  deps?: OverlayWindowDeps;
}

export function OverlayWindow({ monitorIndex, deps }: OverlayWindowProps) {
  const { ipc, port, nextFrame } = useMemo(() => deps ?? defaultDeps(), [deps]);
  const [session, setSession] = useState<OverlayActivate | null>(null);
  const [highlight, setHighlight] = useState<PhysicalRect | null>(null);
  /** The latest session seen: events of earlier ones are dropped. */
  const latest = useRef(0);

  useEffect(() => {
    document.documentElement.classList.add("overlay-page");
    return () => document.documentElement.classList.remove("overlay-page");
  }, []);

  useEffect(() => {
    const events = port();
    let disposed = false;
    let unlisten: Unlisten[] = [];
    void Promise.all([
      events.onActivate((payload) => {
        if (payload.sessionId < latest.current) return;
        latest.current = payload.sessionId;
        setHighlight(null);
        setSession(payload);
      }),
      events.onHighlight((payload) => {
        if (payload.sessionId === latest.current) setHighlight(payload.rect);
      }),
      events.onDeactivate((payload) => {
        if (payload.sessionId !== latest.current) return;
        setSession(null);
        setHighlight(null);
      }),
    ]).then((handles) => {
      if (disposed) {
        for (const stop of handles) stop();
        return;
      }
      unlisten = handles;
      // Listening: Rust may show this overlay now.
      void ipc.ready();
    });
    return () => {
      disposed = true;
      for (const stop of unlisten) stop();
    };
  }, [ipc, port]);

  // Tell Rust once the session is on screen (after the next paint).
  useEffect(() => {
    if (!session) return;
    nextFrame(() => void ipc.shown(session.sessionId, session.monitor.index));
  }, [ipc, nextFrame, session]);

  // Esc or a right click cancels.
  useEffect(() => {
    if (!session) return;
    const cancel = () => void ipc.cancel(session.sessionId);
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") cancel();
    };
    const onContextMenu = (event: MouseEvent) => {
      event.preventDefault();
      cancel();
    };
    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("contextmenu", onContextMenu);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("contextmenu", onContextMenu);
    };
  }, [ipc, session]);

  return (
    <main
      className="overlay"
      data-testid="OverlayWindow"
      data-monitor={monitorIndex}
      data-mode={session?.mode ?? "idle"}
    >
      {session?.mode === "region" && (
        <RegionSelector key={session.sessionId} session={session} ipc={ipc} nextFrame={nextFrame} />
      )}
      {session?.mode === "window" && (
        <WindowHighlighter
          key={session.sessionId}
          session={session}
          highlight={highlight}
          ipc={ipc}
          nextFrame={nextFrame}
        />
      )}
    </main>
  );
}
