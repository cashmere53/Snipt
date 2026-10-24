// One editor window's store and original image, for the components below `EditorWindow`.
import { createContext, useContext } from "react";
import type { StoreApi } from "zustand/vanilla";
import { useStore } from "zustand";
import type { ImageSource } from "../services/image-source";
import type { EditorStore } from "./editor-store";

export interface EditorContextValue {
  store: StoreApi<EditorStore>;
  source: ImageSource;
  newId(): string;
}

export const EditorContext = createContext<EditorContextValue | null>(null);

export function useEditorContext(): EditorContextValue {
  const value = useContext(EditorContext);
  if (value === null) throw new Error("outside an EditorContext");
  return value;
}

/** A slice of the editor state; the selector must return a stable value (or use `useShallow`). */
export function useEditor<T>(selector: (state: EditorStore) => T): T {
  return useStore(useEditorContext().store, selector);
}
