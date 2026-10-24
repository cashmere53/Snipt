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

/** A window whose handlers the test fires by hand. */
export function fakePort() {
  let closeRequested: ((event: { preventDefault(): void }) => void) | null = null;
  let closeRequest: (() => void) | null = null;
  let flushRequest: ((payload: FlushRequestPayload) => void) | null = null;
  const port = {
    onCloseRequested: vi.fn<WindowPort["onCloseRequested"]>(async (handler) => {
      closeRequested = handler;
      return () => (closeRequested = null);
    }),
    onCloseRequest: vi.fn<WindowPort["onCloseRequest"]>(async (handler) => {
      closeRequest = handler;
      return () => (closeRequest = null);
    }),
    onFlushRequest: vi.fn<WindowPort["onFlushRequest"]>(async (handler) => {
      flushRequest = handler;
      return () => (flushRequest = null);
    }),
    destroy: vi.fn<WindowPort["destroy"]>(async () => {}),
  };
  return {
    port,
    /** The user closes the window; returns whether the close was stopped. */
    userClose() {
      let prevented = false;
      closeRequested?.({ preventDefault: () => (prevented = true) });
      return prevented;
    },
    closeRequest: () => closeRequest?.(),
    flushRequest: (requestId: string) => flushRequest?.({ requestId }),
    listening: () => closeRequested !== null && closeRequest !== null && flushRequest !== null,
  };
}
