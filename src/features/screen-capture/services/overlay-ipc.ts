// Typed calls of the overlay commands. Every call resolves to an `OverlayResult`: a rejected
// invoke becomes `{ ok: false, error }` instead of an exception.
import { invoke as tauriInvoke, type InvokeArgs } from "@tauri-apps/api/core";
import {
  type CommitRegionArgs,
  type MagnifierArgs,
  OVERLAY_COMMANDS,
  OVERLAY_ERROR_KINDS,
  type OverlayErrorDto,
  type PhysicalRect,
  type PointerArgs,
  type SessionArgs,
  type ShownArgs,
} from "@/shared/ipc/screen-capture";

export type Invoke = <T>(cmd: string, args?: InvokeArgs) => Promise<T>;

export type OverlayResult<T> = { ok: true; value: T } | { ok: false; error: OverlayErrorDto };

export interface OverlayIpc {
  ready(): Promise<OverlayResult<null>>;
  shown(sessionId: number, monitorIndex: number): Promise<OverlayResult<null>>;
  /** Raw RGBA bytes of `(2 * radius + 1)²` pixels around the local point. */
  magnifier(
    sessionId: number,
    monitorIndex: number,
    x: number,
    y: number,
    radius: number,
  ): Promise<OverlayResult<Uint8Array>>;
  pointer(
    sessionId: number,
    monitorIndex: number,
    x: number,
    y: number,
  ): Promise<OverlayResult<null>>;
  commitRegion(
    sessionId: number,
    monitorIndex: number,
    rect: PhysicalRect,
  ): Promise<OverlayResult<null>>;
  commitWindow(sessionId: number): Promise<OverlayResult<null>>;
  cancel(sessionId: number): Promise<OverlayResult<null>>;
}

/** Errors that do not come from an overlay command (the IPC itself failed) count as stale. */
export function toOverlayError(error: unknown): OverlayErrorDto {
  if (typeof error === "object" && error !== null) {
    const { kind } = error as Record<string, unknown>;
    if (OVERLAY_ERROR_KINDS.includes(kind as OverlayErrorDto["kind"])) {
      return { kind: kind as OverlayErrorDto["kind"] };
    }
  }
  console.error("overlay command failed", error);
  return { kind: "staleSession" };
}

/** A raw response arrives as an `ArrayBuffer` (or, from some mocks, as a plain array). */
function toBytes(value: unknown): Uint8Array {
  if (value instanceof Uint8Array) return value;
  if (value instanceof ArrayBuffer) return new Uint8Array(value);
  if (Array.isArray(value)) return Uint8Array.from(value as number[]);
  throw new TypeError("the magnifier pixels did not arrive as bytes");
}

export function createOverlayIpc(invoke: Invoke = tauriInvoke): OverlayIpc {
  async function call<T>(
    cmd: string,
    args?: InvokeArgs,
    convert: (value: unknown) => T = (value) => value as T,
  ): Promise<OverlayResult<T>> {
    try {
      return { ok: true, value: convert(await invoke<unknown>(cmd, args)) };
    } catch (error) {
      return { ok: false, error: toOverlayError(error) };
    }
  }

  return {
    ready: () => call<null>(OVERLAY_COMMANDS.ready),
    shown: (sessionId, monitorIndex) =>
      call<null>(OVERLAY_COMMANDS.shown, { sessionId, monitorIndex } satisfies ShownArgs),
    magnifier: (sessionId, monitorIndex, x, y, radius) =>
      call(
        OVERLAY_COMMANDS.magnifier,
        { sessionId, monitorIndex, x, y, radius } satisfies MagnifierArgs,
        toBytes,
      ),
    pointer: (sessionId, monitorIndex, x, y) =>
      call<null>(OVERLAY_COMMANDS.pointer, { sessionId, monitorIndex, x, y } satisfies PointerArgs),
    commitRegion: (sessionId, monitorIndex, rect) =>
      call<null>(OVERLAY_COMMANDS.commitRegion, {
        sessionId,
        monitorIndex,
        rect,
      } satisfies CommitRegionArgs),
    commitWindow: (sessionId) =>
      call<null>(OVERLAY_COMMANDS.commitWindow, { sessionId } satisfies SessionArgs),
    cancel: (sessionId) => call<null>(OVERLAY_COMMANDS.cancel, { sessionId } satisfies SessionArgs),
  };
}
