import { lazy, Suspense, useEffect } from "react";
import { resolveWindowRoute } from "./window-route";

// Each spec replaces its placeholder by exporting the real component from its feature entry.
// --- app-shell ---
const SettingsWindow = lazy(() =>
  import("@/features/app-shell").then((m) => ({ default: m.SettingsWindow })),
);
// --- capture-library ---
const HistoryWindow = lazy(() =>
  import("@/features/capture-library").then((m) => ({ default: m.HistoryWindow })),
);
// --- annotation-editor ---
const EditorWindow = lazy(() =>
  import("@/features/annotation-editor").then((m) => ({ default: m.EditorWindow })),
);
// --- screen-capture ---
const OverlayWindow = lazy(() =>
  import("@/features/screen-capture").then((m) => ({ default: m.OverlayWindow })),
);

export function WindowRouter({ label }: { label: string }) {
  const route = resolveWindowRoute(label);

  useEffect(() => {
    if (route.kind === "unknown") {
      console.error(`Unknown window label: ${route.label}`);
    }
  }, [route]);

  return <Suspense fallback={null}>{renderRoute(route)}</Suspense>;
}

function renderRoute(route: ReturnType<typeof resolveWindowRoute>) {
  switch (route.kind) {
    case "settings":
      return <SettingsWindow />;
    case "history":
      return <HistoryWindow />;
    case "editor":
      return <EditorWindow captureId={route.captureId} />;
    case "overlay":
      return <OverlayWindow monitorIndex={route.monitorIndex} />;
    case "unknown":
      return null;
  }
}
