// The state of one editor window: the document's history, selection, tool, style, view, crop
// draft, text being typed and save status. A vanilla zustand store, so the autosave can
// subscribe outside React. It never calls services: whoever needs the style stored or the
// document saved subscribes.
import { createStore, type StoreApi } from "zustand/vanilla";
import type { AnnotationDocument, Point, Rect, Shape, ShapeId, TextShape } from "../model/document";
import { toIntegerRect } from "../model/geometry";
import { commit, createHistory, type History, redo, undo } from "../model/history";
import { commitText, deleteShapes, setCrop } from "../model/operations";
import type { StyleSettings } from "../model/style";
import {
  fitView,
  nextZoomStep,
  previousZoomStep,
  type ViewState,
  type Viewport,
  viewportCenter,
  zoomAround,
} from "./view";

export type ToolId = "select" | "arrow" | "rect" | "text" | "mosaic" | "crop" | "marker";

export type SaveStatus =
  | { kind: "clean" }
  | { kind: "dirty" }
  | { kind: "saving" }
  | { kind: "failed"; message: string };

/** `shapeId` is `null` for a new text. */
export interface TextEditState {
  shapeId: ShapeId | null;
  position: Point;
  draft: string;
}

export interface EditorState {
  history: History;
  /** Bumped by every document change, undo and redo included. */
  revision: number;
  /** The revision whose document was last saved. */
  savedRevision: number;
  selection: readonly ShapeId[];
  tool: ToolId;
  /** The style of new shapes; also applied to the selection. */
  style: StyleSettings;
  view: ViewState;
  viewport: Viewport;
  devicePixelRatio: number;
  /** The crop being adjusted; `null` when not cropping. */
  cropDraft: Rect | null;
  textEdit: TextEditState | null;
  saveStatus: SaveStatus;
}

export interface EditorActions {
  setTool(tool: ToolId): void;
  select(ids: readonly ShapeId[]): void;
  /** Records `next` as one undo step; the same document records nothing. */
  commitDoc(next: AnnotationDocument): void;
  /** Becomes the style of new shapes, and is applied to the selection as one undo step. */
  applyStyle(patch: Partial<StyleSettings>): void;
  deleteSelected(): void;
  beginTextEdit(target: { shapeId: ShapeId } | { position: Point }): void;
  updateTextDraft(draft: string): void;
  /** An empty text creates nothing, or deletes the text being edited. */
  commitTextEdit(): void;
  beginCrop(): void;
  updateCropDraft(rect: Rect): void;
  confirmCrop(): void;
  cancelCrop(): void;
  clearCrop(): void;
  undo(): void;
  redo(): void;
  /** `anchor` (CSS pixels in the view) stays over the same image point; default the center. */
  setZoom(zoom: number, anchor?: Point): void;
  zoomIn(anchor?: Point): void;
  zoomOut(anchor?: Point): void;
  zoomActual(): void;
  zoomFit(): void;
  setPan(pan: Point): void;
  setViewport(size: Viewport, devicePixelRatio: number): void;
  saveStarted(): void;
  saveSucceeded(revision: number): void;
  saveFailed(message: string): void;
}

export type EditorStore = EditorState & EditorActions;

export interface EditorStoreOptions {
  doc: AnnotationDocument;
  style: StyleSettings;
  viewport?: Viewport;
  devicePixelRatio?: number;
  newId?: () => string;
}

const STYLE_TARGETS: Record<keyof StyleSettings, readonly Shape["kind"][]> = {
  color: ["arrow", "rect", "text", "marker"],
  strokeWidth: ["arrow", "rect"],
  fontSize: ["text"],
  textBackground: ["text"],
};

function styled(shape: Shape, patch: Partial<StyleSettings>): Shape {
  let next = shape;
  for (const [key, value] of Object.entries(patch) as [keyof StyleSettings, unknown][]) {
    if (value === undefined || !STYLE_TARGETS[key].includes(shape.kind)) continue;
    const field = key === "textBackground" ? "background" : key;
    if ((next as unknown as Record<string, unknown>)[field] !== value) {
      next = { ...next, [field]: value } as Shape;
    }
  }
  return next;
}

/** What the view shows: the whole image while cropping, else the crop or the whole image. */
function shownArea(state: EditorState): Rect {
  const doc = state.history.present;
  const whole = { x: 0, y: 0, ...doc.image };
  return state.cropDraft === null ? (doc.crop ?? whole) : whole;
}

/** Keeps a fitted view fitted after a change of the shown area or the viewport. */
function refit(state: EditorState): Partial<EditorState> {
  return state.view.mode === "fit"
    ? { view: fitView(shownArea(state), state.viewport, state.devicePixelRatio) }
    : {};
}

const afterChange = (status: SaveStatus): SaveStatus =>
  status.kind === "saving" ? status : { kind: "dirty" };

export function createEditorStore(options: EditorStoreOptions): StoreApi<EditorStore> {
  const newId = options.newId ?? (() => crypto.randomUUID());

  return createStore<EditorStore>()((set, get) => {
    /** Moves the history and everything that depends on the present document. */
    function moveHistory(history: History): void {
      const state = get();
      if (history === state.history) return;
      const ids = new Set(history.present.shapes.map((shape) => shape.id));
      const next: EditorState = {
        ...state,
        history,
        revision: state.revision + 1,
        selection: state.selection.filter((id) => ids.has(id)),
        saveStatus: afterChange(state.saveStatus),
      };
      set({ ...next, ...refit(next) });
    }

    const commitDoc = (doc: AnnotationDocument) => moveHistory(commit(get().history, doc));

    const setZoom = (zoom: number, anchor?: Point) => {
      const { view, viewport, devicePixelRatio } = get();
      set({ view: zoomAround(view, zoom, anchor ?? viewportCenter(viewport), devicePixelRatio) });
    };

    const viewport = options.viewport ?? { width: 0, height: 0 };
    const devicePixelRatio = options.devicePixelRatio ?? 1;
    const whole = { x: 0, y: 0, ...options.doc.image };

    return {
      history: createHistory(options.doc),
      revision: 0,
      savedRevision: 0,
      selection: [],
      tool: "select",
      style: options.style,
      view: fitView(options.doc.crop ?? whole, viewport, devicePixelRatio),
      viewport,
      devicePixelRatio,
      cropDraft: null,
      textEdit: null,
      saveStatus: { kind: "clean" },

      setTool: (tool) => {
        if (tool !== "crop" && get().cropDraft !== null) get().cancelCrop();
        set({ tool, selection: tool === "select" ? get().selection : [] });
      },

      select: (ids) => set({ selection: [...ids] }),

      commitDoc,

      applyStyle: (patch) => {
        const { style, selection, history } = get();
        set({ style: { ...style, ...patch } });
        if (selection.length === 0) return;
        const selected = new Set(selection);
        const doc = history.present;
        let changed = false;
        const shapes = doc.shapes.map((shape) => {
          if (!selected.has(shape.id)) return shape;
          const next = styled(shape, patch);
          changed ||= next !== shape;
          return next;
        });
        if (changed) commitDoc({ ...doc, shapes });
      },

      deleteSelected: () => {
        const { selection, history } = get();
        if (selection.length === 0) return;
        commitDoc(deleteShapes(history.present, selection));
        set({ selection: [] });
      },

      beginTextEdit: (target) => {
        if (get().textEdit !== null) get().commitTextEdit();
        if ("position" in target) {
          set({ textEdit: { shapeId: null, position: target.position, draft: "" } });
          return;
        }
        const shape = get().history.present.shapes.find(
          (s): s is TextShape => s.id === target.shapeId && s.kind === "text",
        );
        if (shape !== undefined) {
          set({ textEdit: { shapeId: shape.id, position: shape.position, draft: shape.text } });
        }
      },

      updateTextDraft: (draft) => {
        const { textEdit } = get();
        if (textEdit !== null) set({ textEdit: { ...textEdit, draft } });
      },

      commitTextEdit: () => {
        const { textEdit, history, style } = get();
        if (textEdit === null) return;
        set({ textEdit: null });
        const doc = history.present;
        const existing =
          textEdit.shapeId === null
            ? undefined
            : doc.shapes.find(
                (s): s is TextShape => s.id === textEdit.shapeId && s.kind === "text",
              );
        const shape: TextShape = existing
          ? { ...existing, text: textEdit.draft }
          : {
              id: newId(),
              kind: "text",
              position: textEdit.position,
              text: textEdit.draft,
              color: style.color,
              fontSize: style.fontSize,
              background: style.textBackground,
            };
        commitDoc(commitText(doc, shape));
      },

      beginCrop: () => {
        const doc = get().history.present;
        const next = { ...get(), cropDraft: doc.crop ?? { x: 0, y: 0, ...doc.image } };
        set({ cropDraft: next.cropDraft, selection: [], ...refit(next) });
      },

      updateCropDraft: (rect) => {
        if (get().cropDraft !== null) set({ cropDraft: rect });
      },

      confirmCrop: () => {
        const { cropDraft, history } = get();
        if (cropDraft === null) return;
        const doc = history.present;
        const rect = toIntegerRect(cropDraft, doc.image);
        const isWhole =
          rect !== null && rect.width === doc.image.width && rect.height === doc.image.height;
        set({ cropDraft: null, tool: "select" });
        if (rect === null) {
          set(refit(get()));
          return;
        }
        const crop = isWhole ? null : rect;
        const same =
          crop === doc.crop ||
          (crop !== null &&
            doc.crop !== null &&
            crop.x === doc.crop.x &&
            crop.y === doc.crop.y &&
            crop.width === doc.crop.width &&
            crop.height === doc.crop.height);
        if (same) set(refit(get()));
        else commitDoc(setCrop(doc, crop));
      },

      cancelCrop: () => {
        if (get().cropDraft === null) return;
        const next = { ...get(), cropDraft: null };
        set({ cropDraft: null, tool: "select", ...refit(next) });
      },

      clearCrop: () => {
        const doc = get().history.present;
        if (doc.crop !== null) commitDoc(setCrop(doc, null));
      },

      undo: () => {
        set({ textEdit: null, cropDraft: null });
        moveHistory(undo(get().history));
      },

      redo: () => {
        set({ textEdit: null, cropDraft: null });
        moveHistory(redo(get().history));
      },

      setZoom,
      zoomIn: (anchor) => setZoom(nextZoomStep(get().view.zoom), anchor),
      zoomOut: (anchor) => setZoom(previousZoomStep(get().view.zoom), anchor),
      zoomActual: () => setZoom(1),
      zoomFit: () => {
        const state = get();
        set({ view: fitView(shownArea(state), state.viewport, state.devicePixelRatio) });
      },
      setPan: (pan) => set({ view: { ...get().view, pan, mode: "manual" } }),

      setViewport: (size, dpr) => {
        const next = { ...get(), viewport: size, devicePixelRatio: dpr };
        set({ viewport: size, devicePixelRatio: dpr, ...refit(next) });
      },

      saveStarted: () => set({ saveStatus: { kind: "saving" } }),
      saveSucceeded: (revision) => {
        const savedRevision = Math.max(get().savedRevision, revision);
        set({
          savedRevision,
          saveStatus: get().revision > savedRevision ? { kind: "dirty" } : { kind: "clean" },
        });
      },
      saveFailed: (message) => set({ saveStatus: { kind: "failed", message } }),
    };
  });
}

export const currentDoc = (state: EditorState): AnnotationDocument => state.history.present;
