// The editor window: loads its capture, then puts the canvas, the panels and the shortcuts
// around one store, saves on its own and before the window goes away.
import { useCallback, useEffect, useMemo, useState } from "react";
import { EditorCanvas } from "./canvas/EditorCanvas";
import { useCloseGuard } from "./hooks/use-close-guard";
import { useShortcuts } from "./hooks/use-shortcuts";
import type { AnnotationDocument } from "./model/document";
import { createAutosave } from "./services/autosave";
import { renderComposite } from "./services/composite-renderer";
import { createEditorIpc, type EditorIpc } from "./services/editor-ipc";
import {
  copyComposite,
  exportComposite,
  type Render,
  suggestedExportName,
} from "./services/export-actions";
import { decodePng, type Drawable, type ImageSource } from "./services/image-source";
import { type LoadedEditor, loadEditor } from "./services/load-editor";
import { loadStyle, saveStyle } from "./services/style-prefs";
import { tauriWindowPort, type WindowPort } from "./services/window-port";
import { EditorContext, useEditorContext } from "./store/editor-context";
import { createEditorStore } from "./store/editor-store";
import { CloseGuardDialog } from "./ui/CloseGuardDialog";
import { type Notice, NoticeBanner } from "./ui/NoticeBanner";
import { StatusBar } from "./ui/StatusBar";
import { StylePanel } from "./ui/StylePanel";
import { Toolbar } from "./ui/Toolbar";
import "./ui/editor.css";

/** What the window talks to; tests replace them. */
export interface EditorWindowDeps {
  ipc: EditorIpc;
  /** Created once the capture is loaded (it needs the Tauri window). */
  windowPort(): WindowPort;
  decode(png: Uint8Array): Promise<Drawable>;
  newId(): string;
}

const defaultDeps = (): EditorWindowDeps => ({
  ipc: createEditorIpc(),
  windowPort: tauriWindowPort,
  decode: decodePng,
  newId: () => crypto.randomUUID(),
});

type Loading = { kind: "loading" } | { kind: "done"; loaded: LoadedEditor };

export function EditorWindow({
  captureId,
  deps: given,
}: {
  captureId: string;
  deps?: EditorWindowDeps;
}) {
  const deps = useMemo(() => given ?? defaultDeps(), [given]);
  const [loading, setLoading] = useState<Loading>({ kind: "loading" });

  useEffect(() => {
    let current = true;
    void loadEditor(deps.ipc, deps.decode).then((loaded) => {
      if (current) setLoading({ kind: "done", loaded });
    });
    return () => {
      current = false;
    };
  }, [deps]);

  if (loading.kind === "loading") {
    return <main className="editor" data-testid="EditorWindow" aria-busy="true" />;
  }
  const { loaded } = loading;
  if (!loaded.ok) {
    return (
      <main className="editor editor-load-failed" data-testid="EditorWindow">
        <h1>キャプチャ {captureId} を開けませんでした</h1>
        <p>{loaded.message}</p>
        <button type="button" onClick={() => void deps.windowPort().destroy().catch(console.error)}>
          閉じる
        </button>
      </main>
    );
  }
  return (
    <EditorSession
      captureId={captureId}
      doc={loaded.doc}
      source={loaded.source}
      warnings={loaded.warnings}
      deps={deps}
    />
  );
}

interface SessionProps {
  captureId: string;
  doc: AnnotationDocument;
  source: ImageSource;
  warnings: string[];
  deps: EditorWindowDeps;
}

function EditorSession({ captureId, doc, source, warnings, deps }: SessionProps) {
  const { newId } = deps;
  const [store] = useState(() =>
    createEditorStore({
      doc,
      style: loadStyle(),
      devicePixelRatio: window.devicePixelRatio || 1,
      newId,
    }),
  );
  const context = useMemo(() => ({ store, source, newId }), [store, source, newId]);

  // The last style is shared with the next editor window.
  useEffect(
    () =>
      store.subscribe((state, previous) => {
        if (state.style !== previous.style) saveStyle(state.style);
      }),
    [store],
  );

  const render: Render = useCallback(
    (current, format) => renderComposite(current, source, format),
    [source],
  );
  const autosave = useMemo(
    () =>
      createAutosave({ store, compose: (current) => render(current, "png"), save: deps.ipc.save }),
    [store, render, deps.ipc],
  );
  useEffect(() => autosave.start(), [autosave]);

  return (
    <EditorContext.Provider value={context}>
      <EditorLayout
        captureId={captureId}
        warnings={warnings}
        render={render}
        flush={autosave.flush}
        deps={deps}
      />
    </EditorContext.Provider>
  );
}

interface LayoutProps {
  captureId: string;
  warnings: string[];
  render: Render;
  flush: ReturnType<typeof createAutosave>["flush"];
  deps: EditorWindowDeps;
}

function EditorLayout({ captureId, warnings, render, flush, deps }: LayoutProps) {
  const { store } = useEditorContext();
  const { ipc } = deps;
  const [port] = useState(deps.windowPort);
  const [notices, setNotices] = useState<Notice[]>(() =>
    warnings.map((message, id) => ({ id, level: "warning", message })),
  );

  const notifyFailure = useCallback((message: string | null) => {
    if (message === null) return;
    setNotices((current) => [
      ...current,
      { id: Math.max(-1, ...current.map((n) => n.id)) + 1, level: "error", message },
    ]);
  }, []);

  const onCopy = useCallback(() => {
    void copyComposite(store.getState().history.present, render, ipc).then(notifyFailure);
  }, [store, render, ipc, notifyFailure]);
  const onSaveAs = useCallback(() => {
    const doc = store.getState().history.present;
    void exportComposite(doc, render, ipc, suggestedExportName(captureId)).then(notifyFailure);
  }, [store, render, ipc, captureId, notifyFailure]);
  useShortcuts({ onCopy, onSaveAs });

  const guard = useCloseGuard({ store, flush, port, flushResult: ipc.flushResult });

  return (
    <main className="editor" data-testid="EditorWindow">
      <Toolbar onCopy={onCopy} onSaveAs={onSaveAs} />
      <StylePanel />
      <NoticeBanner
        notices={notices}
        onDismiss={(id) => setNotices((current) => current.filter((n) => n.id !== id))}
      />
      <EditorCanvas />
      <StatusBar onRetry={() => void flush()} />
      {guard.failure !== null && (
        <CloseGuardDialog message={guard.failure} onRetry={guard.retry} onDiscard={guard.discard} />
      )}
    </main>
  );
}
