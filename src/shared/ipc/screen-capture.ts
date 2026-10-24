// IPC contract owned by screen-capture. Mirrors the Rust types one to one
// (`screen_capture::commands` and `screen_capture::overlay`). Only types and names live here;
// `features/screen-capture/services/overlay-ipc.ts` wraps the calls.
//
// Every command must come from an `overlay-*` window. Coordinates are physical pixels: local
// to the calling overlay's monitor in commands, global (the virtual desktop) in events.

/** Command names, in the order they are listed in `build.rs`. */
export const OVERLAY_COMMANDS = {
  /** No arguments → `null`. The overlay has loaded. */
  ready: "capture_overlay_ready",
  /** `{ sessionId, monitorIndex }` → `null`. The overlay has drawn the session. */
  shown: "capture_overlay_shown",
  /** `MagnifierArgs` → raw RGBA bytes of `(2 * radius + 1)²` pixels. */
  magnifier: "capture_overlay_magnifier",
  /** `PointerArgs` → `null`; the highlight comes back as `HIGHLIGHT_EVENT`. */
  pointer: "capture_overlay_pointer",
  /** `CommitRegionArgs` → `null`. */
  commitRegion: "capture_overlay_commit_region",
  /** `{ sessionId }` → `null`. */
  commitWindow: "capture_overlay_commit_window",
  /** `{ sessionId }` → `null`. */
  cancel: "capture_overlay_cancel",
} as const;

export type OverlayCommand = (typeof OVERLAY_COMMANDS)[keyof typeof OVERLAY_COMMANDS];

/** `PhysicalRect`: covers `x..x + width` and `y..y + height`. */
export interface PhysicalRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface PhysicalPoint {
  x: number;
  y: number;
}

export type OverlayMode = "region" | "window";

export interface OverlayMonitor {
  index: number;
  /** Global physical pixels. */
  rect: PhysicalRect;
  /** CSS px × scale = physical px. */
  scale: number;
}

export interface SessionArgs {
  sessionId: number;
}

export interface ShownArgs extends SessionArgs {
  monitorIndex: number;
}

/** The largest magnifier radius Rust accepts. */
export const MAX_MAGNIFIER_RADIUS = 16;

export interface MagnifierArgs extends SessionArgs {
  monitorIndex: number;
  x: number;
  y: number;
  radius: number;
}

export interface PointerArgs extends SessionArgs {
  monitorIndex: number;
  x: number;
  y: number;
}

export interface CommitRegionArgs extends SessionArgs {
  monitorIndex: number;
  rect: PhysicalRect;
}

export const OVERLAY_ERROR_KINDS = [
  "tooSmall",
  "noWindowHighlighted",
  "staleSession",
  "topologyChanged",
  "outOfRange",
  "notAnOverlay",
] as const;

export type OverlayErrorKind = (typeof OVERLAY_ERROR_KINDS)[number];

export interface OverlayErrorDto {
  kind: OverlayErrorKind;
}

/** Show this overlay for a session. Older sessions' events are ignored. */
export const ACTIVATE_EVENT = "overlay://activate";
/** The window under the pointer (window mode), sent to every overlay. */
export const HIGHLIGHT_EVENT = "overlay://highlight";
/** The session ended: drop its state. */
export const DEACTIVATE_EVENT = "overlay://deactivate";

export interface OverlayActivate {
  sessionId: number;
  mode: OverlayMode;
  monitor: OverlayMonitor;
}

export interface OverlayHighlight {
  sessionId: number;
  /** Global physical pixels; `null` when no window is under the pointer. */
  rect: PhysicalRect | null;
}

export interface OverlayDeactivate {
  sessionId: number;
}

export interface OverlayEvents {
  [ACTIVATE_EVENT]: OverlayActivate;
  [HIGHLIGHT_EVENT]: OverlayHighlight;
  [DEACTIVATE_EVENT]: OverlayDeactivate;
}
