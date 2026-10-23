// Undo and redo over whole-document snapshots.
import type { AnnotationDocument } from "./document";

export interface History {
  past: readonly AnnotationDocument[];
  present: AnnotationDocument;
  future: readonly AnnotationDocument[];
}

/** Undo steps kept (requirement 9.3 asks for at least 100). */
export const HISTORY_LIMIT = 200;

export function createHistory(doc: AnnotationDocument): History {
  return { past: [], present: doc, future: [] };
}

/** Records `next` as one step and drops the redo steps; the same document records nothing. */
export function commit(history: History, next: AnnotationDocument): History {
  if (next === history.present) {
    return history;
  }
  const past = [...history.past, history.present];
  return { past: past.slice(-HISTORY_LIMIT), present: next, future: [] };
}

export function undo(history: History): History {
  const previous = history.past.at(-1);
  if (previous === undefined) {
    return history;
  }
  return {
    past: history.past.slice(0, -1),
    present: previous,
    future: [history.present, ...history.future],
  };
}

export function redo(history: History): History {
  const [next, ...future] = history.future;
  if (next === undefined) {
    return history;
  }
  return { past: [...history.past, history.present], present: next, future };
}

export const canUndo = (history: History) => history.past.length > 0;
export const canRedo = (history: History) => history.future.length > 0;
