/**
 * Maps a Tauri window label to the view it should render.
 * Labels: `settings`, `history`, `editor-<captureId>`, `overlay-<build>-<monitorIndex>`
 * (a new build number each time the overlays are rebuilt).
 */
export type WindowRoute =
  | { kind: "settings" }
  | { kind: "history" }
  | { kind: "editor"; captureId: string }
  | { kind: "overlay"; monitorIndex: number }
  | { kind: "unknown"; label: string };

const EDITOR_PREFIX = "editor-";
const OVERLAY_PREFIX = "overlay-";

export function resolveWindowRoute(label: string): WindowRoute {
  if (label === "settings") return { kind: "settings" };
  if (label === "history") return { kind: "history" };

  if (label.startsWith(EDITOR_PREFIX)) {
    const captureId = label.slice(EDITOR_PREFIX.length);
    if (captureId.length > 0) return { kind: "editor", captureId };
  }

  if (label.startsWith(OVERLAY_PREFIX)) {
    const rest = label.slice(OVERLAY_PREFIX.length);
    const match = /^\d+-(\d+)$/.exec(rest);
    if (match) return { kind: "overlay", monitorIndex: Number(match[1]) };
  }

  return { kind: "unknown", label };
}
