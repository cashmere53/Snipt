// IPC contract owned by annotation-editor. Mirrors the Rust types one to one (see the
// annotation-editor design). Only types and names live here; `services/editor-ipc.ts` wraps
// the calls.
//
// Every command runs on the calling `editor-<captureId>` window: the capture is taken from
// the window label, never from an argument.

/** Command names, in the order they are listed in `build.rs`. */
export const EDITOR_COMMANDS = {
  /** No arguments → `EditorSessionDto`. */
  openSession: "editor_open_session",
  /** No arguments → the original PNG as raw bytes. */
  readOriginal: "editor_read_original",
  /** Raw body: the save envelope (see `save-envelope.ts`) → `null`. */
  save: "editor_save",
  /** Raw body: the composite PNG → `null`. */
  copy: "editor_copy",
  /** `{ suggestedName }` → `ExportTargetDto`, or `null` when the dialog was cancelled. */
  pickExportPath: "editor_pick_export_path",
  /** Raw body: PNG or JPEG, with the `EXPORT_TOKEN_HEADER` header → `null`. */
  writeExport: "editor_write_export",
  /** `FlushResultArgs` → `null`. Answers a `FLUSH_REQUEST_EVENT`. */
  flushResult: "editor_flush_result",
} as const;

export type EditorCommand = (typeof EDITOR_COMMANDS)[keyof typeof EDITOR_COMMANDS];

/** Request header of `editor_write_export` carrying `ExportTargetDto.token` from `editor_pick_export_path`. */
export const EXPORT_TOKEN_HEADER = "x-snipt-export-token";

export interface EditorSessionDto {
  captureId: string;
  /** Physical pixels of the original image. */
  width: number;
  height: number;
  /** RFC 3339 with the offset. */
  capturedAt: string;
  windowTitle: string | null;
  /** An `AnnotationDocument` candidate; checked by `parse-document`. */
  annotations: unknown;
  /** Unreadable annotations, or the capture is not saved yet. */
  warning: string | null;
  /** Not in the save folder yet; the first annotation save retries saving the original. */
  unsaved: boolean;
}

export type ExportFormat = "png" | "jpeg";

export interface PickExportPathArgs {
  suggestedName: string;
}

export interface ExportTargetDto {
  token: string;
  format: ExportFormat;
  fileName: string;
}

export const EDITOR_ERROR_KINDS = [
  "notFound",
  "invalidCaptureId",
  "invalidPayload",
  "library",
  "clipboard",
  "dialog",
  "io",
  "unknownToken",
] as const;

export type EditorErrorKind = (typeof EDITOR_ERROR_KINDS)[number];

/** `message` is the user-facing text. */
export interface EditorErrorDto {
  kind: EditorErrorKind;
  message: string;
}

export type IpcResult<T> = { ok: true; value: T } | { ok: false; error: EditorErrorDto };

/**
 * Rust asks the window to flush its annotations and close (the capture is being deleted, or
 * the editor is closed from outside). The window closes whatever the outcome.
 */
export const CLOSE_REQUEST_EVENT = "editor://close-request";

/**
 * The app is about to quit: flush the annotations but keep the window open, then answer with
 * `editor_flush_result` and the same `requestId`. Rust waits at most 3 seconds.
 */
export const FLUSH_REQUEST_EVENT = "editor://flush-request";

export interface FlushRequestPayload {
  requestId: string;
}

export interface FlushResultArgs {
  requestId: string;
  /** Whether every change has been saved. */
  saved: boolean;
}

export interface EditorEvents {
  [CLOSE_REQUEST_EVENT]: null;
  [FLUSH_REQUEST_EVENT]: FlushRequestPayload;
}
