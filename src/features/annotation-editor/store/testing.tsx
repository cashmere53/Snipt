// Renders editor components with a fresh store and a stand-in image, for tests.
import { render } from "@testing-library/react";
import type { ReactNode } from "react";
import type { AnnotationDocument } from "../model/document";
import { emptyDocument } from "../model/document";
import { DEFAULT_STYLE, type StyleSettings } from "../model/style";
import { ImageSource } from "../services/image-source";
import { EditorContext } from "./editor-context";
import { createEditorStore } from "./editor-store";

export function renderWithEditor(
  ui: ReactNode,
  options: { doc?: Partial<AnnotationDocument>; style?: StyleSettings } = {},
) {
  let next = 0;
  const newId = () => `id-${++next}`;
  const store = createEditorStore({
    doc: { ...emptyDocument({ width: 400, height: 300 }), ...options.doc },
    style: options.style ?? DEFAULT_STYLE,
    viewport: { width: 800, height: 600 },
    newId,
  });
  const source = new ImageSource({ width: 400, height: 300 } as never);
  const result = render(
    <EditorContext.Provider value={{ store, source, newId }}>{ui}</EditorContext.Provider>,
  );
  return { ...result, store, state: () => store.getState() };
}
