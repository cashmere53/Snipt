// Typed calls of the editor commands. Every call resolves to an `IpcResult`: a rejected
// invoke becomes `{ ok: false, error }` instead of an exception.
import { invoke as tauriInvoke, type InvokeArgs, type InvokeOptions } from "@tauri-apps/api/core";
import {
  EDITOR_COMMANDS,
  EDITOR_ERROR_KINDS,
  type EditorErrorDto,
  type EditorSessionDto,
  EXPORT_TOKEN_HEADER,
  type ExportTargetDto,
  type FlushResultArgs,
  type IpcResult,
  type PickExportPathArgs,
} from "@/shared/ipc/annotation-editor";

export type Invoke = <T>(cmd: string, args?: InvokeArgs, options?: InvokeOptions) => Promise<T>;

export interface EditorIpc {
  openSession(): Promise<IpcResult<EditorSessionDto>>;
  readOriginal(): Promise<IpcResult<Uint8Array>>;
  /** `envelope` from `encodeSaveEnvelope`. */
  save(envelope: Uint8Array): Promise<IpcResult<null>>;
  copy(png: Uint8Array): Promise<IpcResult<null>>;
  /** `null` when the dialog was cancelled. */
  pickExportPath(suggestedName: string): Promise<IpcResult<ExportTargetDto | null>>;
  writeExport(token: string, bytes: Uint8Array): Promise<IpcResult<null>>;
  flushResult(requestId: string, saved: boolean): Promise<IpcResult<null>>;
}

function isEditorError(value: unknown): value is EditorErrorDto {
  if (typeof value !== "object" || value === null) return false;
  const { kind, message } = value as Record<string, unknown>;
  return EDITOR_ERROR_KINDS.includes(kind as EditorErrorDto["kind"]) && typeof message === "string";
}

/** Errors that do not come from an editor command (the IPC itself failed) count as `io`. */
export function toEditorError(error: unknown): EditorErrorDto {
  if (isEditorError(error)) return error;
  let message: string;
  if (typeof error === "string") message = error;
  else if (error instanceof Error) message = error.message;
  else message = String(JSON.stringify(error));
  return { kind: "io", message };
}

/** A raw response arrives as an `ArrayBuffer` (or, from some mocks, as a plain array). */
function toBytes(value: unknown): Uint8Array {
  if (value instanceof Uint8Array) return value;
  if (value instanceof ArrayBuffer) return new Uint8Array(value);
  if (Array.isArray(value)) return Uint8Array.from(value as number[]);
  throw new TypeError("the original image did not arrive as bytes");
}

export function createEditorIpc(invoke: Invoke = tauriInvoke): EditorIpc {
  async function call<T>(
    cmd: string,
    args?: InvokeArgs,
    options?: InvokeOptions,
    convert: (value: unknown) => T = (value) => value as T,
  ): Promise<IpcResult<T>> {
    try {
      return { ok: true, value: convert(await invoke<unknown>(cmd, args, options)) };
    } catch (error) {
      return { ok: false, error: toEditorError(error) };
    }
  }

  return {
    openSession: () => call<EditorSessionDto>(EDITOR_COMMANDS.openSession),
    readOriginal: () => call(EDITOR_COMMANDS.readOriginal, undefined, undefined, toBytes),
    save: (envelope) => call<null>(EDITOR_COMMANDS.save, envelope),
    copy: (png) => call<null>(EDITOR_COMMANDS.copy, png),
    pickExportPath: (suggestedName) =>
      call<ExportTargetDto | null>(EDITOR_COMMANDS.pickExportPath, {
        suggestedName,
      } satisfies PickExportPathArgs),
    writeExport: (token, bytes) =>
      call<null>(EDITOR_COMMANDS.writeExport, bytes, {
        headers: { [EXPORT_TOKEN_HEADER]: token },
      }),
    flushResult: (requestId, saved) =>
      call<null>(EDITOR_COMMANDS.flushResult, {
        requestId,
        saved,
      } satisfies FlushResultArgs as unknown as InvokeArgs),
  };
}
