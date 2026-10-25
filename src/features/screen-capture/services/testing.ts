// Fakes of the overlay's IPC, events and animation frames, for tests.
import { vi } from "vitest";
import type {
  OverlayActivate,
  OverlayDeactivate,
  OverlayErrorDto,
  OverlayHighlight,
  OverlayMode,
} from "@/shared/ipc/screen-capture";
import type { OverlayWindowDeps } from "../OverlayWindow";
import type { OverlayIpc, OverlayResult } from "./overlay-ipc";
import type { OverlayPort } from "./overlay-port";

export const ok = <T>(value: T): OverlayResult<T> => ({ ok: true, value });
export const fail = (kind: OverlayErrorDto["kind"]): OverlayResult<never> => ({
  ok: false,
  error: { kind },
});

/** A 400 × 300 physical px monitor at 150% (266.67 × 200 CSS px), right of the primary. */
export function activate(sessionId: number, mode: OverlayMode): OverlayActivate {
  return {
    sessionId,
    mode,
    monitor: { index: 1, rect: { x: 1920, y: 0, width: 400, height: 300 }, scale: 1.5 },
  };
}

/** Every command succeeds unless overridden; calls are recorded by the `vi.fn`s. */
export function fakeIpc() {
  return {
    ready: vi.fn<OverlayIpc["ready"]>(async () => ok(null)),
    shown: vi.fn<OverlayIpc["shown"]>(async () => ok(null)),
    magnifier: vi.fn<OverlayIpc["magnifier"]>(async (_s, _m, _x, _y, radius) =>
      ok(new Uint8Array((2 * radius + 1) ** 2 * 4)),
    ),
    pointer: vi.fn<OverlayIpc["pointer"]>(async () => ok(null)),
    commitRegion: vi.fn<OverlayIpc["commitRegion"]>(async () => ok(null)),
    commitWindow: vi.fn<OverlayIpc["commitWindow"]>(async () => ok(null)),
    cancel: vi.fn<OverlayIpc["cancel"]>(async () => ok(null)),
  };
}

/** Rust's side of the events: the test sends them by hand. */
export function fakePort() {
  const handlers: {
    activate?: (payload: OverlayActivate) => void;
    highlight?: (payload: OverlayHighlight) => void;
    deactivate?: (payload: OverlayDeactivate) => void;
  } = {};
  const port: OverlayPort = {
    onActivate: async (handler) => {
      handlers.activate = handler;
      return () => (handlers.activate = undefined);
    },
    onHighlight: async (handler) => {
      handlers.highlight = handler;
      return () => (handlers.highlight = undefined);
    },
    onDeactivate: async (handler) => {
      handlers.deactivate = handler;
      return () => (handlers.deactivate = undefined);
    },
  };
  return {
    port,
    activate: (payload: OverlayActivate) => handlers.activate?.(payload),
    highlight: (payload: OverlayHighlight) => handlers.highlight?.(payload),
    deactivate: (payload: OverlayDeactivate) => handlers.deactivate?.(payload),
  };
}

/** Animation frames that run only when the test says so. */
export function fakeFrames() {
  let queue: (() => void)[] = [];
  return {
    nextFrame: (callback: () => void) => {
      queue.push(callback);
    },
    pending: () => queue.length,
    run: () => {
      const callbacks = queue;
      queue = [];
      for (const callback of callbacks) callback();
    },
  };
}

export function fakeDeps() {
  const ipc = fakeIpc();
  const events = fakePort();
  const frames = fakeFrames();
  const deps: OverlayWindowDeps = { ipc, port: () => events.port, nextFrame: frames.nextFrame };
  return { ipc, events, frames, deps };
}
