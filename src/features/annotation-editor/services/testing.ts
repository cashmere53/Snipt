// Fakes of the editor's IPC and window, for tests.
import { vi } from "vitest";
import type {
  EditorErrorDto,
  EditorSessionDto,
  ExportTargetDto,
  FlushRequestPayload,
  IpcResult,
} from "@/shared/ipc/annotation-editor";
import type { EditorIpc } from "./editor-ipc";
import type { WindowPort } from "./window-port";

export const ok = <T>(value: T): IpcResult<T> => ({ ok: true, value });
export const fail = (kind: EditorErrorDto["kind"], message: string): IpcResult<never> => ({
  ok: false,
  error: { kind, message },
});

export function sessionDto(overrides: Partial<EditorSessionDto> = {}): EditorSessionDto {
  return {
    captureId: "20261022_174150",
    width: 400,
    height: 300,
    capturedAt: "2026-10-22T17:41:50+09:00",
    windowTitle: null,
    annotations: null,
    warning: null,
    unsaved: false,
    ...overrides,
  };
}

/** Every command succeeds unless overridden; calls are recorded by the `vi.fn`s. */
export function fakeIpc(overrides: Partial<EditorIpc> = {}) {
  const ipc = {
    openSession: vi.fn<EditorIpc["openSession"]>(async () => ok(sessionDto())),
    readOriginal: vi.fn<EditorIpc["readOriginal"]>(async () => ok(new Uint8Array([1]))),
    save: vi.fn<EditorIpc["save"]>(async () => ok(null)),
    copy: vi.fn<EditorIpc["copy"]>(async () => ok(null)),
    pickExportPath: vi.fn<EditorIpc["pickExportPath"]>(async () =>
      ok<ExportTargetDto | null>({ token: "t", format: "png", fileName: "a.png" }),
    ),
    writeExport: vi.fn<EditorIpc["writeExport"]>(async () => ok(null)),
    flushResult: vi.fn<EditorIpc["flushResult"]>(async () => ok(null)),
  };
  for (const [name, implementation] of Object.entries(overrides)) {
    ipc[name as keyof EditorIpc].mockImplementation(implementation as never);
  }
  return ipc;
}

/** Subscribes `handler` to a set of live handlers; unlisten removes only that handler. */
function listen<T>(live: Set<T>): (handler: T) => Promise<() => void> {
  return async (handler) => {
    live.add(handler);
    return () => void live.delete(handler);
  };
}

/** A window whose handlers the test fires by hand, like Tauri: every live handler gets each event. */
export function fakePort() {
  const live = {
    closeRequested: new Set<(event: { preventDefault(): void }) => void>(),
    closeRequest: new Set<() => void>(),
    flushRequest: new Set<(payload: FlushRequestPayload) => void>(),
  };
  const port = {
    onCloseRequested: vi.fn<WindowPort["onCloseRequested"]>(listen(live.closeRequested)),
    onCloseRequest: vi.fn<WindowPort["onCloseRequest"]>(listen(live.closeRequest)),
    onFlushRequest: vi.fn<WindowPort["onFlushRequest"]>(listen(live.flushRequest)),
    destroy: vi.fn<WindowPort["destroy"]>(async () => {}),
  };
  return {
    port,
    /** The user closes the window; returns whether the close was stopped. */
    userClose() {
      let prevented = false;
      for (const handler of live.closeRequested) {
        handler({ preventDefault: () => (prevented = true) });
      }
      return prevented;
    },
    closeRequest: () => {
      for (const handler of live.closeRequest) handler();
    },
    flushRequest: (requestId: string) => {
      for (const handler of live.flushRequest) handler({ requestId });
    },
    listening: () =>
      live.closeRequested.size > 0 && live.closeRequest.size > 0 && live.flushRequest.size > 0,
    /** How many handlers are subscribed: one of each kind, never two. */
    subscriptions: () => live.closeRequested.size + live.closeRequest.size + live.flushRequest.size,
  };
}
