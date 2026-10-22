// Shows how feature tests mock the Tauri runtime: window labels via mockWindows, commands via mockIPC.
import { invoke } from "@tauri-apps/api/core";
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { describe, expect, it } from "vitest";

describe("Tauri mocks", () => {
  it("provides the current window label", () => {
    mockWindows("editor-abc");
    expect(getCurrentWindow().label).toBe("editor-abc");
  });

  it("intercepts commands", async () => {
    mockIPC((cmd, args) => (cmd === "echo" ? args : undefined));
    await expect(invoke("echo", { value: 1 })).resolves.toEqual({ value: 1 });
  });
});
