import { mockIPC } from "@tauri-apps/api/mocks";
import { describe, expect, it } from "vitest";
import {
  type Accelerator,
  hotkeysResume,
  hotkeysRetry,
  hotkeysStatus,
  hotkeysSuspend,
  type Settings,
  settingsCheckSaveDir,
  settingsGet,
  settingsUpdate,
} from "./app-shell";

const settings: Settings = {
  hotkeys: {
    region: "PrintScreen" as Accelerator,
    window: "Ctrl+PrintScreen" as Accelerator,
    fullscreen: "Shift+PrintScreen" as Accelerator,
  },
  fullscreenTarget: "cursorMonitor",
  saveDir: "C:\\Users\\me\\Pictures\\Snipt",
  autostart: true,
};

function recordCalls() {
  const calls: { cmd: string; args: unknown }[] = [];
  mockIPC((cmd, args) => {
    calls.push({ cmd, args });
    return cmd === "settings_get" || cmd === "settings_update" ? settings : [];
  });
  return calls;
}

describe("app-shell IPC wrappers", () => {
  it("call settings commands with the documented names and arguments", async () => {
    const calls = recordCalls();
    await expect(settingsGet()).resolves.toEqual(settings);
    await expect(settingsUpdate(settings)).resolves.toEqual(settings);
    await settingsCheckSaveDir("D:\\shots");
    expect(calls).toEqual([
      { cmd: "settings_get", args: {} },
      { cmd: "settings_update", args: { next: settings } },
      { cmd: "settings_check_save_dir", args: { path: "D:\\shots" } },
    ]);
  });

  it("call hotkey commands without arguments", async () => {
    const calls = recordCalls();
    await hotkeysStatus();
    await hotkeysRetry();
    await hotkeysSuspend();
    await hotkeysResume();
    expect(calls.map((c) => c.cmd)).toEqual([
      "hotkeys_status",
      "hotkeys_retry",
      "hotkeys_suspend",
      "hotkeys_resume",
    ]);
  });

  it("passes a rejected SettingsUpdateError through", async () => {
    mockIPC(() => {
      throw { kind: "persistFailed", message: "disk full" };
    });
    await expect(settingsUpdate(settings)).rejects.toEqual({
      kind: "persistFailed",
      message: "disk full",
    });
  });
});
