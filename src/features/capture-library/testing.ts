// Test helpers for the history window: capture fixtures and a command mock that records calls.
import { mockIPC } from "@tauri-apps/api/mocks";
import type { CaptureId, CaptureItem } from "@/shared/ipc/capture-library";

export const captureId = (s: string) => s as CaptureId;

/** A capture at 2026-10-22 17:41:<second>, newest first when listed by descending second. */
export function captureFixture(second: number, overrides: Partial<CaptureItem> = {}): CaptureItem {
  const ss = String(second).padStart(2, "0");
  return {
    id: captureId(`20261022_1741${ss}`),
    capturedAt: `2026-10-22T17:41:${ss}+09:00`,
    mode: "region",
    width: 1920,
    height: 1080,
    windowTitle: null,
    originalPath: `C:\\shots\\Snipt_20261022_1741${ss}.png`,
    annotatedPath: null,
    hasAnnotations: false,
    thumbnailPath: `C:\\shots\\.snipt\\thumbs\\20261022_1741${ss}.png`,
    thumbnailVersion: 1,
    ...overrides,
  };
}

type Handler = (args: Record<string, unknown>) => unknown;

/**
 * Mocks commands by name (unlisted commands resolve to `undefined`) and records every call.
 * Events are mocked too, so tests can `emit` library events.
 */
export function mockLibrary(handlers: Record<string, Handler>) {
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
    callsTo: (cmd: string) => calls.filter((call) => call.cmd === cmd).map((call) => call.args),
  };
}
