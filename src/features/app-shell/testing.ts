// Test helpers for the settings window: a settings fixture and a command mock that records calls.
import { mockIPC } from "@tauri-apps/api/mocks";
import type { Accelerator, HotkeyStatus, Settings } from "@/shared/ipc/app-shell";

export function settingsFixture(): Settings {
  return {
    hotkeys: {
      region: "PrintScreen" as Accelerator,
      window: "Ctrl+PrintScreen" as Accelerator,
      fullscreen: "Shift+PrintScreen" as Accelerator,
    },
    fullscreenTarget: "cursorMonitor",
    saveDir: "C:\\Users\\me\\Pictures\\Snipt",
    autostart: true,
  };
}

export function registeredStatuses(settings: Settings): HotkeyStatus[] {
  return (["region", "window", "fullscreen"] as const).map((mode) => ({
    mode,
    accelerator: settings.hotkeys[mode],
    registered: true,
    failure: null,
  }));
}

type Handler = (args: Record<string, unknown>) => unknown;

/**
 * Mocks commands by name (unlisted commands resolve to `undefined`) and records every call.
 * A handler that throws makes the command reject with the thrown value.
 */
export function mockCommands(handlers: Record<string, Handler>) {
  const calls: { cmd: string; args: Record<string, unknown> }[] = [];
  mockIPC(
    (cmd, args) => {
      const payload = (args ?? {}) as Record<string, unknown>;
      calls.push({ cmd, args: payload });
      return handlers[cmd]?.(payload);
    },
    { shouldMockEvents: true },
  );
  return {
    calls,
    names: () => calls.map((call) => call.cmd).filter((cmd) => !cmd.startsWith("plugin:event")),
  };
}
