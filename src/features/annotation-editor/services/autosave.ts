// Saves the annotations 1 second after the last change, one save at a time, and on demand
// (`flush`) when the window closes or the app quits. A failed save is not retried by itself:
// the next change or `flush` tries again.
import type { StoreApi } from "zustand/vanilla";
import type { IpcResult } from "@/shared/ipc/annotation-editor";
import type { AnnotationDocument } from "../model/document";
import type { EditorStore } from "../store/editor-store";
import { type RenderResult, renderErrorMessage } from "./composite-renderer";
import { encodeSaveEnvelope } from "./save-envelope";

export const AUTOSAVE_DELAY_MS = 1_000;

export type SaveResult = { ok: true } | { ok: false; message: string };

export interface AutosaveDeps {
  store: StoreApi<EditorStore>;
  /** The composite PNG of `doc` (`renderComposite(doc, source, "png")`). */
  compose(doc: AnnotationDocument): Promise<RenderResult>;
  /** `editor_save` with the envelope. */
  save(envelope: Uint8Array): Promise<IpcResult<null>>;
  delayMs?: number;
}

export interface AutosaveController {
  /** Follows the store's changes; returns the function that stops it. */
  start(): () => void;
  /** Saves now if anything is unsaved, after the save in progress. */
  flush(): Promise<SaveResult>;
}

export function createAutosave(deps: AutosaveDeps): AutosaveController {
  const { store } = deps;
  const delay = deps.delayMs ?? AUTOSAVE_DELAY_MS;
  let timer: ReturnType<typeof setTimeout> | null = null;
  let running: Promise<SaveResult> | null = null;
  /** The timer fired during a save: save again once it is done. */
  let again = false;

  const unsaved = () => store.getState().revision > store.getState().savedRevision;

  function cancelTimer(): void {
    if (timer !== null) clearTimeout(timer);
    timer = null;
  }

  function schedule(): void {
    cancelTimer();
    timer = setTimeout(() => {
      timer = null;
      void run();
    }, delay);
  }

  async function saveOnce(): Promise<SaveResult> {
    const { revision, history, saveStarted } = store.getState();
    saveStarted();
    let failure: string;
    try {
      const composite = await deps.compose(history.present);
      if (composite.ok) {
        const result = await deps.save(encodeSaveEnvelope(history.present, composite.bytes));
        if (result.ok) {
          store.getState().saveSucceeded(revision);
          return { ok: true };
        }
        failure = result.error.message;
      } else {
        failure = renderErrorMessage(composite.error);
      }
    } catch (error) {
      failure = error instanceof Error ? error.message : String(error);
    }
    store.getState().saveFailed(failure);
    return { ok: false, message: failure };
  }

  function run(): Promise<SaveResult> {
    if (running !== null) {
      again = true;
      return running;
    }
    if (!unsaved()) return Promise.resolve({ ok: true });
    const current = saveOnce().then((result) => {
      running = null;
      if (again) {
        again = false;
        if (result.ok && unsaved()) schedule();
      }
      return result;
    });
    running = current;
    return current;
  }

  return {
    start() {
      const unsubscribe = store.subscribe((state, previous) => {
        if (state.revision !== previous.revision && unsaved()) schedule();
      });
      return () => {
        unsubscribe();
        cancelTimer();
      };
    },

    async flush() {
      cancelTimer();
      if (running !== null) {
        await running;
        // The save in progress decides nothing about what came after it.
        cancelTimer();
        again = false;
      }
      return run();
    },
  };
}
