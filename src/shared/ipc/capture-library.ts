// IPC contract owned by capture-library. Mirrors the Rust types one to one (see the
// capture-library design).
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { CaptureMode } from "./app-shell";

/** `"YYYYMMDD_HHMMSS"` or `"YYYYMMDD_HHMMSS_NN"`. Only values from Rust carry the brand. */
export type CaptureId = string & { readonly __brand: "CaptureId" };

export interface CaptureItem {
  id: CaptureId;
  /** RFC 3339 with the offset. */
  capturedAt: string;
  mode: CaptureMode;
  /** Physical pixels. */
  width: number;
  height: number;
  windowTitle: string | null;
  originalPath: string;
  annotatedPath: string | null;
  hasAnnotations: boolean;
  thumbnailPath: string | null;
  thumbnailVersion: number;
}

export type LibraryErrorKind =
  | "notFound"
  | "saveDirUnavailable"
  | "diskFull"
  | "permissionDenied"
  | "invalidImage"
  | "invalidAnnotationDocument"
  | "clipboard"
  | "shell"
  | "io";

/** `message` is the user-facing text. */
export interface LibraryError {
  kind: LibraryErrorKind;
  message: string;
  id?: CaptureId;
  path?: string;
}

export interface DeleteOutcome {
  removed: CaptureId[];
  failed: { id: CaptureId; error: LibraryError }[];
}

export type ItemsRemovedReason = "deleted" | "missingOnDisk";

export interface LibraryEvents {
  "library://item-added": { item: CaptureItem };
  "library://item-updated": { item: CaptureItem };
  "library://items-removed": { ids: CaptureId[]; reason: ItemsRemovedReason };
  "library://reloaded": { saveDir: string };
}

/** Newest first. */
export const listCaptures = (): Promise<CaptureItem[]> => invoke("library_list");
export const openInEditor = (id: CaptureId): Promise<void> =>
  invoke("library_open_in_editor", { id });
/** Rejects with a `LibraryError`. */
export const copyCapture = (id: CaptureId): Promise<void> => invoke("library_copy", { id });
/** Rejects with a `LibraryError`. */
export const revealCapture = (id: CaptureId): Promise<void> => invoke("library_reveal", { id });
export const deleteCaptures = (ids: CaptureId[]): Promise<DeleteOutcome> =>
  invoke("library_delete", { ids });
/** Resolves with the ids that were actually missing and removed. */
export const pruneMissing = (ids: CaptureId[]): Promise<CaptureId[]> =>
  invoke("library_prune_missing", { ids });

/** The thumbnail URL with a version query so a regenerated file is reloaded, or null. */
export function thumbnailSrc(item: CaptureItem): string | null {
  if (item.thumbnailPath === null) {
    return null;
  }
  return `${convertFileSrc(item.thumbnailPath)}?v=${item.thumbnailVersion}`;
}

export function onLibraryEvent<K extends keyof LibraryEvents>(
  name: K,
  handler: (payload: LibraryEvents[K]) => void,
): Promise<UnlistenFn> {
  return listen<LibraryEvents[K]>(name, (event) => handler(event.payload));
}
