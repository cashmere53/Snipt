import { describe, expect, it, vi } from "vitest";
import { createOverlayIpc, type Invoke } from "./overlay-ipc";

type Call = (cmd: string, args?: unknown) => Promise<unknown>;

describe("createOverlayIpc", () => {
  it("sends each command with its arguments", async () => {
    const invoke = vi.fn<Call>().mockResolvedValue(null);
    const ipc = createOverlayIpc(invoke as Invoke);
    await ipc.ready();
    await ipc.shown(3, 1);
    await ipc.pointer(3, 1, 10, 20);
    await ipc.commitRegion(3, 1, { x: 1, y: 2, width: 3, height: 4 });
    await ipc.commitWindow(3);
    await ipc.cancel(3);
    expect(invoke.mock.calls).toEqual([
      ["capture_overlay_ready", undefined],
      ["capture_overlay_shown", { sessionId: 3, monitorIndex: 1 }],
      ["capture_overlay_pointer", { sessionId: 3, monitorIndex: 1, x: 10, y: 20 }],
      [
        "capture_overlay_commit_region",
        { sessionId: 3, monitorIndex: 1, rect: { x: 1, y: 2, width: 3, height: 4 } },
      ],
      ["capture_overlay_commit_window", { sessionId: 3 }],
      ["capture_overlay_cancel", { sessionId: 3 }],
    ]);
  });

  it("turns the magnifier answer into bytes", async () => {
    const invoke = vi.fn<Call>().mockResolvedValue(new Uint8Array([1, 2, 3, 4]).buffer);
    const result = await createOverlayIpc(invoke as Invoke).magnifier(3, 1, 5, 6, 8);
    expect(invoke).toHaveBeenCalledWith("capture_overlay_magnifier", {
      sessionId: 3,
      monitorIndex: 1,
      x: 5,
      y: 6,
      radius: 8,
    });
    expect(result).toEqual({ ok: true, value: new Uint8Array([1, 2, 3, 4]) });
  });

  it("returns the error kind, or stale for an IPC failure", async () => {
    const tooSmall = vi.fn<Call>().mockRejectedValue({ kind: "tooSmall" });
    expect(
      await createOverlayIpc(tooSmall as Invoke).commitRegion(1, 0, {
        x: 0,
        y: 0,
        width: 0,
        height: 1,
      }),
    ).toEqual({
      ok: false,
      error: { kind: "tooSmall" },
    });
    vi.spyOn(console, "error").mockImplementation(() => {});
    const broken = vi.fn<Call>().mockRejectedValue("state not managed");
    expect(await createOverlayIpc(broken as Invoke).cancel(1)).toEqual({
      ok: false,
      error: { kind: "staleSession" },
    });
  });
});
