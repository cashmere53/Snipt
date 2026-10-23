// IPC contract owned by app-shell. Mirrors the Rust types one to one (see the app-shell design).
import { invoke } from "@tauri-apps/api/core";

export type CaptureMode = "region" | "window" | "fullscreen";
export type FullscreenTarget = "cursorMonitor" | "allMonitors";

/** Normalized accelerator such as `"Ctrl+PrintScreen"`. Only values from Rust or the hotkey input carry the brand. */
export type Accelerator = string & { readonly __brand: "Accelerator" };

export interface HotkeySettings {
  region: Accelerator;
  window: Accelerator;
  fullscreen: Accelerator;
}

export interface Settings {
  hotkeys: HotkeySettings;
  fullscreenTarget: FullscreenTarget;
  saveDir: string;
  autostart: boolean;
}

export type HotkeyFailureKind = "conflict" | "printScreenReservedByWindows";

export interface HotkeyStatus {
  mode: CaptureMode;
  accelerator: Accelerator;
  registered: boolean;
  failure: HotkeyFailureKind | null;
}

export type InvalidHotkeyReason = "noKey" | "modifierRequired" | "unknownKey";

export type SettingsUpdateError =
  | { kind: "duplicateHotkey"; accelerator: Accelerator; modes: CaptureMode[] }
  | { kind: "invalidHotkey"; mode: CaptureMode; reason: InvalidHotkeyReason }
  | { kind: "saveDirNotWritable"; path: string; message: string }
  | { kind: "hotkeyRegistrationFailed"; statuses: HotkeyStatus[] }
  | { kind: "autostartFailed"; message: string }
  | { kind: "persistFailed"; message: string };

/** Emitted to every window after settings are committed. Payload: `Settings`. */
export const SETTINGS_CHANGED_EVENT = "settings://changed" as const;

export const settingsGet = (): Promise<Settings> => invoke("settings_get");

/** Rejects with a `SettingsUpdateError` when nothing was changed. */
export const settingsUpdate = (next: Settings): Promise<Settings> =>
  invoke("settings_update", { next });

/** Rejects with a `saveDirNotWritable` error. */
export const settingsCheckSaveDir = (path: string): Promise<void> =>
  invoke("settings_check_save_dir", { path });

export const hotkeysStatus = (): Promise<HotkeyStatus[]> => invoke("hotkeys_status");
export const hotkeysRetry = (): Promise<HotkeyStatus[]> => invoke("hotkeys_retry");
export const hotkeysSuspend = (): Promise<void> => invoke("hotkeys_suspend");
export const hotkeysResume = (): Promise<void> => invoke("hotkeys_resume");
